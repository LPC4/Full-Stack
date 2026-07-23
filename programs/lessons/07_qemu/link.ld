/* Put everything in one chunk at 0x80000000, where virt's RAM starts. */

ENTRY(_start)

SECTIONS
{
  . = 0x80000000;
  .text   : { *(.text*)   }
  .rodata : { *(.rodata*) }
  .data   : { *(.data*)   }
  .bss    : { *(.bss*) *(COMMON) }
}
