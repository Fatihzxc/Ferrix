//! TikOS — bare-metal Rust on the Zynq-7000 (Cortex-A9).
//!
//! Milestone 2: print over **Zynq UART1** instead of semihosting.
//!
//! UART1 is the Cadence UART that most Zynq dev boards (and the QEMU
//! `xilinx-zynq-a9` machine model) wire to the user-facing serial line.
//! The same code that runs here on QEMU will run unchanged when we load
//! this ELF over OpenOCD onto the real chip — there's no QEMU-only
//! semihosting hop in this version.

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

// ---------- Zynq UART register map (UG585 ch.33 / Appendix B) ----------
//
// Absolute base addresses (UG585 p.1775, register summary table):
//   UART0 = 0xE0000000    UART1 = 0xE0001000
//
// On QEMU's UART model, writing to FIFO sends bytes immediately. On real
// silicon we will eventually need to set baud rate, mode, and enable TX —
// but the on-board boot ROM / our future bootloader will leave the UART
// in a usable state for the first revision.

const UART0_BASE: usize = 0xE000_0000; // UG585 p.1775
const UART1_BASE: usize = 0xE000_1000; // UG585 p.1775

// Register offsets (UG585 p.1775, register summary table).
const UART_OFF_CR: usize = 0x00;      // Control Register         UG585 p.1776-1777
const UART_OFF_MR: usize = 0x04;      // Mode Register            UG585 p.1777-1778
const UART_OFF_BAUDGEN: usize = 0x18; // Baud Rate Generator (CD) UG585 p.1785
const UART_OFF_SR: usize = 0x2C;      // Channel Status Register  UG585 p.1786-1787
const UART_OFF_FIFO: usize = 0x30;    // TX/RX FIFO               UG585 p.1790
const UART_OFF_BAUDDIV: usize = 0x34; // Baud Rate Divider (BDIV) UG585 p.1792

// Control Register bit fields (UG585 p.1776-1777, XUARTPS_CR_OFFSET).
//   bit 0: RXRST     bit 1: TXRST     bit 2: RXEN    bit 3: RXDIS
//   bit 4: TXEN      bit 5: TXDIS     bit 6: TORST   bit 7: STARTBRK   bit 8: STOPBRK
const CR_RXRST: u32 = 1 << 0;  // Software reset for Rx data path (self-clearing)
const CR_TXRST: u32 = 1 << 1;  // Software reset for Tx data path (self-clearing)
const CR_RXEN: u32 = 1 << 2;   // Receive enable
const CR_RXDIS: u32 = 1 << 3;  // Receive disable
const CR_TXEN: u32 = 1 << 4;   // Transmit enable
const CR_TXDIS: u32 = 1 << 5;  // Transmit disable

// Channel Status Register bit fields (UG585 p.1786-1787, XUARTPS_SR_OFFSET).
const SR_TXFULL: u32 = 1 << 4; // TX FIFO full — poll before writing FIFO

// Baud-rate config (UG585 p.1785 BAUDGEN + p.1792 BAUDDIV):
//   baud = uart_ref_clk / (CD * (BDIV + 1))
// In QSPI boot mode, BootROM brings up the IO PLL and configures
// SLCR.UART_CLK_CTRL so uart_ref_clk = 100 MHz.
//   CD=124, BDIV=6  ->  100_000_000 / (124 * 7) = 115207 baud (0.006% err).
// Pure JTAG-park BootROM mode leaves uart_ref_clk = 50 MHz; halve CD to 62
// in that scenario.
const BAUD_CD: u32 = 124;    // BAUDGEN CD value, bits [15:0], reset=0x28B
const BAUD_BDIV: u32 = 6;    // BAUDDIV BDIV value, bits [7:0], reset=0xF

/// Initialise a Zynq UART so writes to its FIFO actually transmit.
/// On QEMU this just enables TX (the chardev backend ignores baud-rate
/// programming). On real silicon this also sets 115200 8N1 explicitly,
/// so we don't depend on whatever the BootROM left in the registers.
fn uart_init(base: usize) {
    let cr = (base + UART_OFF_CR) as *mut u32;
    let mr = (base + UART_OFF_MR) as *mut u32;
    let baudgen = (base + UART_OFF_BAUDGEN) as *mut u32;
    let bauddiv = (base + UART_OFF_BAUDDIV) as *mut u32;
    unsafe {
        // MR: 8N1, normal mode (UG585 p.1777-1778).
        //   bits [9:8] CHMODE   = 00   (normal)
        //   bits [7:6] NBSTOP   = 00   (1 stop bit)
        //   bits [5:3] PAR      = 100  (no parity)
        //   bits [2:1] CHRL     = 00   (8 bits)
        //   bit  [0]   CLKSEL   = 0    (uart_ref_clk, no /8 prescale)
        //   => 0b000_00_100_00_0 = 0x20
        write_volatile(mr, 0x20);
        // Baud-rate generator + divider — see BAUD_CD / BAUD_BDIV above.
        write_volatile(baudgen, BAUD_CD);
        write_volatile(bauddiv, BAUD_BDIV);
        // Reset TX/RX paths, then enable both. TXDIS / RXDIS bits are
        // explicitly NOT set, so the _EN bits stick.
        // TXRST / RXRST are self-clearing — they reset the data paths once.
        write_volatile(cr, CR_TXRST | CR_RXRST | CR_TXEN | CR_RXEN);
    }
}

fn uart_putc_at(base: usize, c: u8) {
    let sr = (base + UART_OFF_SR) as *const u32;
    let fifo = (base + UART_OFF_FIFO) as *mut u32;
    unsafe {
        // Bounded spin so a wedged UART doesn't deadlock the kernel.
        let mut budget: u32 = 1_000_000;
        while (read_volatile(sr) & SR_TXFULL) != 0 && budget > 0 {
            budget -= 1;
        }
        write_volatile(fifo, c as u32);
    }
}

// ---------- boot stub ----------
//
// `_start` runs first (set by the linker). It:
//   1. loads `sp` with the top of the stack region the linker reserved,
//   2. zeroes the .bss section so static-zero-initialised data really is zero,
//   3. branches into Rust at `kmain`.
//
// `bl kmain` instead of `b kmain` so we keep a sane link register; if kmain
// ever returned (it won't — its type is `!`) we'd at least not jump to junk.
// Boot stub.
//
// On a real Zynq the chip arrives with whatever the BootROM / U-Boot left
// behind: MMU on, caches on, page tables we know nothing about. We can't
// reach our own peripherals through someone else's virtual address space,
// so step one is to put the CPU into a known clean state — MMU off, both
// caches off, TLBs flushed — before anything else.
//
// On QEMU `-kernel` the CPU starts with MMU/caches off already, so these
// SCTLR writes are no-ops there. Same code, both worlds.
//
// .bss zeroing is still parked. Once we add a real BSS-resident static
// we'll re-introduce a verified zero loop.
global_asm!(
    r#"
    .section .text._start, "ax"
    .global _start
    _start:
        // -- 1. Disable MMU + I/D caches (clear M, C, I bits in SCTLR) --
        //    ARM ARM v7-A: SCTLR = p15, 0, <Rd>, c1, c0, 0
        //    Bit 0 (M)=MMU enable, Bit 2 (C)=D-cache, Bit 12 (I)=I-cache
        mrc     p15, 0, r0, c1, c0, 0       @ r0 = SCTLR
        bic     r0, r0, #(1 << 0)           @ M  = 0  (MMU off)
        bic     r0, r0, #(1 << 2)           @ C  = 0  (D-cache off)
        bic     r0, r0, #(1 << 12)          @ I  = 0  (I-cache off)
        mcr     p15, 0, r0, c1, c0, 0       @ SCTLR = r0
        isb

        // -- 2. Invalidate all TLBs and the I-cache --
        //    ARM ARM: TLBIALL = p15, 0, <Rd>, c8, c7, 0
        //    ARM ARM: ICIALLU = p15, 0, <Rd>, c7, c5, 0
        mov     r0, #0
        mcr     p15, 0, r0, c8, c7, 0       @ TLBIALL (invalidate all TLBs)
        mcr     p15, 0, r0, c7, c5, 0       @ ICIALLU (invalidate entire I-cache)
        dsb
        isb

        // -- 3. Set up a stack and enter Rust --
        ldr     sp, =_stack_top
        bl      kmain

    1:  wfe
        b       1b
    "#
);

// ---------- UART driver ----------

/// Send one byte over **both** Zynq UARTs.
/// QEMU's chardev mapping decides which one becomes the visible terminal;
/// on real hardware, the on-board USB-UART is wired to UART1. Writing both
/// keeps the same binary portable while we figure out the routing.
fn uart_putc(c: u8) {
    uart_putc_at(UART0_BASE, c);
    uart_putc_at(UART1_BASE, c);
}

fn uart_puts(s: &[u8]) {
    for &b in s {
        uart_putc(b);
    }
}

/// Render a u32 as decimal ASCII into `buf`, return the length written.
/// Used so we can print a tick counter without pulling in core::fmt.
fn u32_to_dec(mut n: u32, buf: &mut [u8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    let mut j = 0;
    while i > 0 {
        i -= 1;
        buf[j] = tmp[i];
        j += 1;
    }
    j
}

fn print_u32(n: u32) {
    let mut buf = [0u8; 10];
    let len = u32_to_dec(n, &mut buf);
    uart_puts(&buf[..len]);
}

// Crude busy-wait. Real timing comes later when we set up the Cortex-A9
// generic timer or the Zynq private timer.
fn delay_busy(loops: u32) {
    for _ in 0..loops {
        unsafe { asm!("nop") };
    }
}

// ---------- kernel main ----------

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    uart_init(UART0_BASE);
    uart_init(UART1_BASE);
    uart_puts(b"TikOS: hello from Cortex-A9 over UART1\n");
    uart_puts(b"  (no semihosting; bytes travel through the actual Cadence UART model.)\n");

    let mut tick: u32 = 0;
    while tick < 5 {
        delay_busy(2_000_000);
        uart_puts(b"  tick ");
        print_u32(tick);
        uart_puts(b"\n");
        tick = tick.wrapping_add(1);
    }

    uart_puts(b"TikOS: done. Ctrl-A then X to quit QEMU.\n");
    loop {
        unsafe { asm!("wfe") };
    }
}

#[panic_handler]
fn on_panic(_info: &PanicInfo) -> ! {
    uart_puts(b"TikOS PANIC\n");
    loop {
        unsafe { asm!("wfe") };
    }
}
