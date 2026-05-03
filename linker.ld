/* TikOS minimal linker script for Zynq-7000 / Cortex-A9 under QEMU.
 *
 * Layout assumes QEMU `-M xilinx-zynq-a9 -kernel <elf>`:
 *   - QEMU loads the ELF at its declared addresses
 *   - QEMU jumps to the ENTRY symbol
 *   - DRAM starts at 0x00000000; we place TikOS at 0x00100000 (1 MiB) so
 *     the low addresses stay free for vector tables / ATAGs / DTB later.
 *
 * For real silicon the bootloader will load us into DDR, not OCM, so
 * 0x00100000 is a fine starting choice for both QEMU and on-board work.
 */

ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0xFFFC0000, LENGTH = 192K
}

SECTIONS
{
    .text :
    {
        /* Vector table must sit at the very start of the kernel image so
         * VBAR = ORIGIN(RAM) = 0xFFFC0000 points at it. The reset entry
         * (offset 0x00) branches to _start, so BootROM's "PC=kernel base"
         * jump still lands correctly. */
        KEEP(*(.text.vectors))
        KEEP(*(.text._start))
        *(.text .text.*)
    } > RAM

    .rodata :
    {
        *(.rodata .rodata.*)
    } > RAM

    .data :
    {
        *(.data .data.*)
    } > RAM

    .bss (NOLOAD) :
    {
        _bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(8);
        _bss_end = .;
    } > RAM

    . = ALIGN(16);
    _stack_bottom = .;
    . += 64K;
    _stack_top = .;

    /DISCARD/ :
    {
        *(.ARM.exidx*)
        *(.ARM.extab*)
    }
}
