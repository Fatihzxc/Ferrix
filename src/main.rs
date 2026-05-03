//! TikOS — bare-metal Rust on the Zynq-7000 (Cortex-A9).
//!
//! Crate root: declares modules, hosts `kmain` + the panic handler.
//!
//! - [`boot`] — `_start` Assembly prologue (CPU bringup: SCTLR clear, TLB invalidate).
//! - [`uart`] — Cadence UART driver (init, puts, print_u32).
//! - [`slcr`] — System-Level Control Registers — register map (helpers added in M2.5 commits).
//!
//! Same binary runs on QEMU (`-M xilinx-zynq-a9 -kernel`) and on real
//! Zynq-7010 silicon over JTAG (OpenOCD + FT232H, see `scripts/`).

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

mod boot;
mod slcr;
mod uart;
mod vectors;

/// Crude busy-wait. Real timing comes later when we set up the
/// Cortex-A9 generic timer or the Zynq private timer.
fn delay_busy(loops: u32) {
    for _ in 0..loops {
        unsafe { asm!("nop") };
    }
}

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    slcr::init();
    uart::init(uart::UART0_BASE);
    uart::init(uart::UART1_BASE);
    uart::puts(b"TikOS: hello from Cortex-A9 over UART1\n");
    uart::puts(b"  (no semihosting; bytes travel through the actual Cadence UART model.)\n");

    let mut tick: u32 = 0;
    while tick < 5 {
        delay_busy(2_000_000);
        uart::puts(b"  tick ");
        uart::print_u32(tick);
        uart::puts(b"\n");
        tick = tick.wrapping_add(1);
    }

    uart::puts(b"TikOS: done. Ctrl-A then X to quit QEMU.\n");

    // M3 vector-table smoke test: trigger an SVC. CPU jumps via VBAR + 0x08
    // to svc_handler, which calls on_svc (UART "TikOS: SVC") and parks in wfe.
    unsafe { asm!("svc #0") };

    loop {
        unsafe { asm!("wfe") };
    }
}

#[panic_handler]
fn on_panic(_info: &PanicInfo) -> ! {
    uart::puts(b"TikOS PANIC\n");
    loop {
        unsafe { asm!("wfe") };
    }
}
