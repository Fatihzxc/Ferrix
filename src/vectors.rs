//! ARMv7-A exception vector table + minimal handlers.
//!
//! Per ARM ARM v7-A §B1.8.1, the vector table has 8 × 4-byte entries:
//!
//! | Offset | Exception |
//! |--------|-----------|
//! | 0x00   | Reset |
//! | 0x04   | Undefined Instruction |
//! | 0x08   | Supervisor Call (SVC) |
//! | 0x0C   | Prefetch Abort |
//! | 0x10   | Data Abort |
//! | 0x14   | Reserved (Hyp on virt-ext, unused for us) |
//! | 0x18   | IRQ |
//! | 0x1C   | FIQ |
//!
//! The base address is selected by `SCTLR.V` (bit 13):
//!   V=0 → low vectors @ `VBAR` (default 0). V=1 → high vectors @ 0xFFFF0000.
//!
//! We use V=0 (default) and program `VBAR = 0xFFFC0000` (kernel base) in
//! `boot.rs` after SCTLR clear. The reset entry is `b _start` so that even
//! if the CPU ever re-enters via offset 0x00, it lands in the same boot
//! stub BootROM uses today.
//!
//! ## Stack note
//!
//! ARMv7-A banks SP per mode. We only initialize `SP_svc` in `_start`
//! (boot.rs). `svc_handler` is safe because the CPU is already in SVC
//! mode when SVC traps fire from kernel code. Other handlers (Undef,
//! Abort, IRQ, FIQ) do not have a valid SP yet — calling Rust from them
//! would corrupt memory. They are stubbed to `wfe` for now; per-mode
//! stacks come with the IRQ work in M4.

use core::arch::global_asm;

use crate::uart;

global_asm!(
    r#"
    .section .text.vectors, "ax"
    .global _vector_table
    _vector_table:
        b _start             @ 0x00 Reset
        b undef_handler      @ 0x04 Undefined Instruction
        b svc_handler        @ 0x08 Supervisor Call
        b pabt_handler       @ 0x0C Prefetch Abort
        b dabt_handler       @ 0x10 Data Abort
        b reserved_handler   @ 0x14 Reserved (Hyp slot)
        b irq_handler        @ 0x18 IRQ
        b fiq_handler        @ 0x1C FIQ

    @ -- SVC handler: CPU was already in SVC mode in our kernel, so
    @    SP_svc is the same stack we used in kmain. Safe to call Rust.
    svc_handler:
        bl on_svc
    1:  wfe
        b 1b

    @ -- The remaining handlers do not yet have a valid SP for their
    @    mode. They print nothing (calling uart::puts would clobber
    @    memory at an undefined SP) and just halt the CPU.
    undef_handler:
    1:  wfe
        b 1b
    pabt_handler:
    1:  wfe
        b 1b
    dabt_handler:
    1:  wfe
        b 1b
    reserved_handler:
    1:  wfe
        b 1b
    irq_handler:
    1:  wfe
        b 1b
    fiq_handler:
    1:  wfe
        b 1b
    "#
);

#[no_mangle]
extern "C" fn on_svc() {
    uart::puts(b"TikOS: SVC\n");
}
