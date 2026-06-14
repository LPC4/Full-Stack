# Separate-compilation headline: this program inlines no I/O. It calls puts/putc/
# exit, all undefined here and resolved by ld against a separately assembled
# stdlib.o (CALL_PLT relocations). `la msg` relocates against this object's .data.
# Prints "hello from stdlib!\n" and exits 42.

.globl _start
.text
_start:
  la a0, msg
  call puts
  li a0, 33          # '!'
  call putc
  li a0, 10          # newline
  call putc
  li a0, 42
  call exit
.data
msg:
  .asciz "hello from stdlib"
