//! Boot stub — `_start` runs first, hands off to `kmain`.
//!
//! On a real Zynq the chip arrives with whatever the BootROM / U-Boot
//! left behind: MMU on, caches on, page tables we know nothing about.
//! We can't reach our own peripherals through someone else's virtual
//! address space, so step one is to put the CPU into a known clean
//! state — MMU off, both caches off, TLBs flushed — before anything
//! else.
//!
//! On QEMU `-kernel` the CPU starts with MMU/caches off already, so
//! these SCTLR writes are no-ops there. Same code, both worlds.
//!
//! `bl kmain` instead of `b kmain` so we keep a sane link register; if
//! kmain ever returned (it won't — its type is `!`) we'd at least not
//! jump to junk.
//!
//! `.bss` zeroing is still parked. Once we add a real BSS-resident
//! static we'll re-introduce a verified zero loop.

use core::arch::global_asm;

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

        // -- 3. Program VBAR to the kernel base (vector table lives here) --
        //    ARM ARM v7-A §B4.1.156: VBAR = p15, 0, <Rt>, c12, c0, 0
        //    SCTLR.V (bit 13) is cleared above, so the CPU consults VBAR
        //    on exception entry instead of the high-vector base 0xFFFF0000.
        ldr     r0, =_vector_table
        mcr     p15, 0, r0, c12, c0, 0      @ VBAR = _vector_table
        isb

        // -- 4. Set up a stack and enter Rust --
        ldr     sp, =_stack_top
        bl      kmain

    1:  wfe
        b       1b
    "#
);
