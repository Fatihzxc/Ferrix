//! Cadence UART (Zynq) driver — register map + minimal init/putc/puts.
//!
//! UART0 base `0xE000_0000`, UART1 base `0xE000_1000`. Output goes to
//! **both** UARTs so the same binary works in QEMU (chardev maps one
//! to stdio) and on real hardware (UART1 wired to the on-board USB-UART
//! through CH340E).
//!
//! Reference: UG585 ch.19 (Cadence UART), Appendix B (register summary).

#![allow(dead_code)]

use core::ptr::{read_volatile, write_volatile};

// ---------- Register map (UG585 ch.19 / Appendix B) ----------

pub const UART0_BASE: usize = 0xE000_0000; // UG585 p.1775
pub const UART1_BASE: usize = 0xE000_1000; // UG585 p.1775

// Register offsets (UG585 p.1775).
const OFF_CR: usize = 0x00;       // Control Register      UG585 p.1776-1777
const OFF_MR: usize = 0x04;       // Mode Register         UG585 p.1777-1778
const OFF_BAUDGEN: usize = 0x18;  // Baud Rate Generator   UG585 p.1785
const OFF_SR: usize = 0x2C;       // Channel Status        UG585 p.1786-1787
const OFF_FIFO: usize = 0x30;     // TX/RX FIFO            UG585 p.1790
const OFF_BAUDDIV: usize = 0x34;  // Baud Rate Divider     UG585 p.1792

// Control Register bit fields (UG585 p.1776-1777).
//   bit 0: RXRST     bit 1: TXRST     bit 2: RXEN    bit 3: RXDIS
//   bit 4: TXEN      bit 5: TXDIS     bit 6: TORST   bit 7: STARTBRK   bit 8: STOPBRK
const CR_RXRST: u32 = 1 << 0;     // self-clearing
const CR_TXRST: u32 = 1 << 1;     // self-clearing
const CR_RXEN: u32 = 1 << 2;
const CR_RXDIS: u32 = 1 << 3;
const CR_TXEN: u32 = 1 << 4;
const CR_TXDIS: u32 = 1 << 5;

// Channel Status Register bit fields (UG585 p.1786-1787).
const SR_TXFULL: u32 = 1 << 4;    // TX FIFO full — poll before writing FIFO

// Baud-rate config (UG585 p.1785 BAUDGEN + p.1792 BAUDDIV):
//   baud = uart_ref_clk / (CD * (BDIV + 1))
// In QSPI boot mode, BootROM brings up the IO PLL and configures
// SLCR.UART_CLK_CTRL so uart_ref_clk = 100 MHz.
//   CD=124, BDIV=6  ->  100_000_000 / (124 * 7) = 115207 baud (0.006% err).
// Pure JTAG-park BootROM mode leaves uart_ref_clk = 50 MHz; halve CD to 62
// in that scenario.
const BAUD_CD: u32 = 124;
const BAUD_BDIV: u32 = 6;

// ---------- Driver ----------

/// Initialise a single UART so writes to its FIFO actually transmit.
/// On QEMU this just enables TX (the chardev backend ignores baud-rate
/// programming). On real silicon this also sets 115200 8N1 explicitly,
/// so we don't depend on whatever the BootROM left in the registers.
pub fn init(base: usize) {
    let cr = (base + OFF_CR) as *mut u32;
    let mr = (base + OFF_MR) as *mut u32;
    let baudgen = (base + OFF_BAUDGEN) as *mut u32;
    let bauddiv = (base + OFF_BAUDDIV) as *mut u32;
    unsafe {
        // MR: 8N1, normal mode (UG585 p.1777-1778).
        //   bits [9:8] CHMODE = 00  (normal)
        //   bits [7:6] NBSTOP = 00  (1 stop bit)
        //   bits [5:3] PAR    = 100 (no parity)
        //   bits [2:1] CHRL   = 00  (8 bits)
        //   bit  [0]   CLKSEL = 0   (uart_ref_clk, no /8 prescale)
        write_volatile(mr, 0x20);
        write_volatile(baudgen, BAUD_CD);
        write_volatile(bauddiv, BAUD_BDIV);
        // Reset TX/RX paths, then enable both. _DIS bits not set,
        // so the _EN bits stick. RXRST/TXRST are self-clearing.
        write_volatile(cr, CR_TXRST | CR_RXRST | CR_TXEN | CR_RXEN);
    }
}

fn putc_at(base: usize, c: u8) {
    let sr = (base + OFF_SR) as *const u32;
    let fifo = (base + OFF_FIFO) as *mut u32;
    unsafe {
        // Bounded spin so a wedged UART doesn't deadlock the kernel.
        let mut budget: u32 = 1_000_000;
        while (read_volatile(sr) & SR_TXFULL) != 0 && budget > 0 {
            budget -= 1;
        }
        write_volatile(fifo, c as u32);
    }
}

/// Send one byte over **both** UARTs.
/// QEMU's chardev mapping decides which becomes the visible terminal;
/// on real hardware UART1 is the user-facing serial line.
fn putc(c: u8) {
    putc_at(UART0_BASE, c);
    putc_at(UART1_BASE, c);
}

/// Send a byte slice over both UARTs.
pub fn puts(s: &[u8]) {
    for &b in s {
        putc(b);
    }
}

/// Render `n` as decimal ASCII into `buf`, return bytes written.
/// Used so we can print a tick counter without pulling in `core::fmt`.
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

/// Print a u32 as decimal ASCII over both UARTs.
pub fn print_u32(n: u32) {
    let mut buf = [0u8; 10];
    let len = u32_to_dec(n, &mut buf);
    puts(&buf[..len]);
}
