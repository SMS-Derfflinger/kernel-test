OUTPUT_ARCH(riscv)
ENTRY(_start)

RAM = 0x80200000;

MEMORY {
    LOWMEM : org = 0x0000000000000000, len = 1M
    KBSS   : org = 0xffffffffc0200000, len = 2M
    KIMAGE : org = 0xffffffff80200000, len = 2M
}

REGION_ALIAS("REGION_TEXT", KIMAGE);
REGION_ALIAS("REGION_RODATA", KIMAGE);
REGION_ALIAS("REGION_BOOT", KIMAGE);
REGION_ALIAS("REGION_DATA", KIMAGE);
REGION_ALIAS("REGION_BSS", KBSS);
REGION_ALIAS("REGION_EHFRAME", KIMAGE);
PROVIDE(_stext = ORIGIN(REGION_TEXT));

SECTIONS {
    .text.dummy (NOLOAD) :
    {
        /*
         * If we use _stext somewhere before its first appearance below, we
         * need to define it as absolute here to avoid linker errors.
         */
        . = ABSOLUTE(_stext);

    } > REGION_TEXT

    .text _stext : AT(RAM) {
        __stext = .;

        *(.text.entry);
        *(.text .text.*);

        . = ALIGN(0x1000);
    } > REGION_TEXT

    __etext = .;

    .rodata : ALIGN(16)
    {
        __srodata = .;

        *(.rodata .rodata.*);

    } > REGION_RODATA

    __erodata = .;

    .data : ALIGN(16)
    {
        __sdata = .;

        *(.data .data.*);
        *(.got .got.plt);

    } > REGION_DATA

    __edata = .;

    .bss (NOLOAD) : ALIGN(16)
    {
        __sbss = .;

        *(.bss .bss.*);

        . = ALIGN(0x1000);
    } > REGION_BSS

    __ebss = .;

    .eh_frame : ALIGN(16)
    {
        __seh_frame = .;

        KEEP(*(.eh_frame .eh_frame*));

    } > REGION_EHFRAME

    . = ALIGN(0x1000);
    __eeh_frame = .;
}

SECTIONS {
    /* Stabs debugging sections.  */
    .stab          0 : { KEEP(*(.stab)); }
    .stabstr       0 : { KEEP(*(.stabstr)); }
    .stab.excl     0 : { KEEP(*(.stab.excl)); }
    .stab.exclstr  0 : { KEEP(*(.stab.exclstr)); }
    .stab.index    0 : { KEEP(*(.stab.index)); }
    .stab.indexstr 0 : { KEEP(*(.stab.indexstr)); }
    .comment       0 : { KEEP(*(.comment)); }
    /* DWARF debug sections.
       Symbols in the DWARF debugging sections are relative to the beginning
       of the section so we begin them at 0.  */
    /* DWARF 1 */
    .debug          0 : { KEEP(*(.debug)); }
    .line           0 : { KEEP(*(.line)); }
    /* GNU DWARF 1 extensions */
    .debug_srcinfo  0 : { KEEP(*(.debug_srcinfo)); }
    .debug_sfnames  0 : { KEEP(*(.debug_sfnames)); }
    /* DWARF 1.1 and DWARF 2 */
    .debug_aranges  0 : { KEEP(*(.debug_aranges)); }
    .debug_pubnames 0 : { KEEP(*(.debug_pubnames)); }
    /* DWARF 2 */
    .debug_info     0 : { KEEP(*(.debug_info)); }
    .debug_abbrev   0 : { KEEP(*(.debug_abbrev)); }
    .debug_line     0 : { KEEP(*(.debug_line)); }
    .debug_frame    0 : { KEEP(*(.debug_frame)); }
    .debug_str      0 : { KEEP(*(.debug_str)); }
    .debug_loc      0 : { KEEP(*(.debug_loc)); }
    .debug_macinfo  0 : { KEEP(*(.debug_macinfo)); }
    /* SGI/MIPS DWARF 2 extensions */
    .debug_weaknames 0 : { KEEP(*(.debug_weaknames)); }
    .debug_funcnames 0 : { KEEP(*(.debug_funcnames)); }
    .debug_typenames 0 : { KEEP(*(.debug_typenames)); }
    .debug_varnames  0 : { KEEP(*(.debug_varnames)); }

    /* DWARF Other */
    .debug_ranges  0 : { KEEP(*(.debug_ranges)); }
    .debug_line_str 0 : { KEEP(*(.debug_line_str)); }

    /DISCARD/ :
    {
        *(.fini_array*)
        *(.note*)
        *(.dtors*)
        *(.debug_gdb_scripts*)
    }
}
SECTIONS {
    .text _stext : AT(RAM) {
        __stext = .;

        *(.text.entry);
        *(.text .text.*);

        . = ALIGN(0x1000);
    } > REGION_TEXT

    __etext = .;
}

SECTIONS {
    .text.syscall_fns :
    {

        KEEP(*(.syscall_fns*));

    } > REGION_TEXT
}
INSERT AFTER .text;

SECTIONS {
    .rodata.fixups :
    {
        . = ALIGN(16);
        FIX_START = .;

        KEEP(*(.fix));

        FIX_END = .;
    } > REGION_RODATA

    .rodata.syscalls :
    {
        . = ALIGN(16);
        __raw_syscall_handlers_start = .;

        RAW_SYSCALL_HANDLERS = .;
        KEEP(*(.raw_syscalls*));

        __raw_syscall_handlers_end = .;

        RAW_SYSCALL_HANDLERS_SIZE =
            ABSOLUTE(__raw_syscall_handlers_end - __raw_syscall_handlers_start);
    } > REGION_RODATA
}
INSERT AFTER .rodata;

SECTIONS {
    .percpu 0 : ALIGN(16)
    {
        __spercpu = .;

        PERCPU_START = .;

        . = ALIGN(16);

        *(.percpu .percpu*);

        . = ALIGN(16);
        __epercpu = .;
    } > LOWMEM AT> REGION_RODATA

    PERCPU_DATA_START = LOADADDR(.percpu);
    PERCPU_LENGTH = ABSOLUTE(__epercpu - __spercpu);

    KIMAGE_PAGES = (__edata - _stext + 0x1000 - 1) / 0x1000;
    KIMAGE_32K_COUNT = (KIMAGE_PAGES + 8 - 1) / 8;
    _ekernel = .;

    BSS_LENGTH = ABSOLUTE(__ebss - __sbss);
}
INSERT AFTER .rodata;

SECTIONS {
    .bootregion : {
        . = ALIGN(4096);
        *(.bootstack);
        *(.bootdata);
    } > REGION_BOOT
}
INSERT AFTER .rodata;
