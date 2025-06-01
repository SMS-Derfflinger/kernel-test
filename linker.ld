# fdt_test/linker.ld
OUTPUT_ARCH(riscv)
ENTRY(_start)

MEMORY
{
  RAM : ORIGIN = 0x80200000, LENGTH = 128M
}

SECTIONS
{
  .text.entry : {
    KEEP(*(.text.entry))
  } > RAM

  .text : {
    *(.text .text.*)
  } > RAM

  .rodata : {
    *(.rodata .rodata.*)
  } > RAM

  .data : {
    *(.data .data.*)
  } > RAM

  .bss : {
    . = ALIGN(4096);
    *(.bss .bss.*)
    *(.bss.stack)
    *(COMMON)
  } > RAM

  /DISCARD/ : {
    *(.eh_frame)
    *(.got)
  }
}
