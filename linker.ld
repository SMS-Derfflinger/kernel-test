# fdt_test/linker.ld
OUTPUT_ARCH(riscv)
ENTRY(_start)

RAM = 0x80200000;

MEMORY
{
  VIRT : ORIGIN = 0xffffffff80200000, LENGTH = 128M
}

SECTIONS
{
  _skernel = .;
  .text.entry : AT(RAM) {
    KEEP(*(.text.entry))
  } > VIRT

  .text : {
    *(.text .text.*)
  } > VIRT

  .rodata : {
    *(.rodata .rodata.*)
  } > VIRT

  .data : {
    *(.data .data.*)
  } > VIRT

  .bss : {
    . = ALIGN(4096);
    *(.bss .bss.*)
    *(.bss.stack)
    *(COMMON)
  } > VIRT
  _ekernel = .;

  /DISCARD/ : {
    *(.eh_frame)
    *(.got)
  }
}
