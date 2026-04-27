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

// ---------- Zynq UART1 register map (subset, from UG585 ch.19) ----------
//
// UART1_BASE = 0xE0001000  (UART0 is at 0xE0000000)
//
// Offsets we use:
//   +0x2C  Channel Status Register (RO)  — bit 4 (TXFULL) is what we poll.
//   +0x30  Channel FIFO              (WO) — write a byte here to transmit.
//
// On QEMU's UART model, writing to FIFO sends bytes immediately. On real
// silicon we will eventually need to set baud rate, mode, and enable TX —
// but the on-board boot ROM / our future bootloader will leave the UART
// in a usable state for the first revision.

const UART0_BASE: usize = 0xE000_0000;
const UART1_BASE: usize = 0xE000_1000;

// Register offsets, per Xilinx UG585 ch.19 (UART register map).
const UART_OFF_CR: usize = 0x00;     // Control Register
const UART_OFF_MR: usize = 0x04;     // Mode Register
const UART_OFF_BAUDGEN: usize = 0x18;// Baud-rate generator (CD)
const UART_OFF_SR: usize = 0x2C;     // Channel Status Register
const UART_OFF_FIFO: usize = 0x30;   // TX/RX FIFO
const UART_OFF_BAUDDIV: usize = 0x34;// Baud-rate divider (BDIV)

// Control Register bits we care about.
const CR_STTBRGEN: u32 = 1 << 1; // start baud-rate generator
const CR_RXEN: u32 = 1 << 4;
const CR_TXEN: u32 = 1 << 6;

// Status Register bits.
const SR_TXFULL: u32 = 1 << 4;

// Baud-rate config: baud = uart_ref_clk / (CD * (BDIV + 1)).
// On this board, U-Boot programmed `uart_ref_clk = 100 MHz` (verified by
// reading SLCR + watching what came out the wire).
//   CD=124, BDIV=6  ->  100_000_000 / (124 * 7) = 115207 baud (0.006% error).
// On a chip in BootROM-default state (uart_ref_clk = 50 MHz, UG585 §25),
// halve CD to 62.
const BAUD_CD: u32 = 124;
const BAUD_BDIV: u32 = 6;

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
        // 8 data bits, 1 stop bit, no parity, normal mode.
        //   bits [4:3]  parity = 0b100 (no parity)
        //   bits [2:1]  char length = 0b00 (8 bits)
        //   bit  [0]    clock select = 0 (uart_ref_clk / 1, no /8 prescale)
        write_volatile(mr, 0b100_00 << 3);
        // Baud-rate generator + divider — see BAUD_CD / BAUD_BDIV above.
        write_volatile(baudgen, BAUD_CD);
        write_volatile(bauddiv, BAUD_BDIV);
        // Enable TX + RX, start baud-rate generator. TXDIS / RXDIS
        // bits are explicitly NOT set, so the corresponding _EN bits stick.
        write_volatile(cr, CR_STTBRGEN | CR_RXEN | CR_TXEN);
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
        mrc     p15, 0, r0, c1, c0, 0       @ r0 = SCTLR
        bic     r0, r0, #(1 << 0)           @ M  = 0  (MMU off)
        bic     r0, r0, #(1 << 2)           @ C  = 0  (D-cache off)
        bic     r0, r0, #(1 << 12)          @ I  = 0  (I-cache off)
        mcr     p15, 0, r0, c1, c0, 0       @ SCTLR = r0
        isb

        // -- 2. Invalidate all TLBs and the I-cache --
        mov     r0, #0
        mcr     p15, 0, r0, c8, c7, 0       @ TLBIALL (all TLBs)
        mcr     p15, 0, r0, c7, c5, 0       @ ICIALLU (whole I-cache)
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
