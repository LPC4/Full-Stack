# Minimal user-space stdlib, assembled and linked separately.
# Exports putc/puts/exit so a program need not inline its own I/O; a cc-compiled
# client (hello.hll) calls putc here instead of cc emitting it inline:
#   cc hello.hll hello.s && as hello.s hello.o && as stdlib.s stdlib.o
#   ld stdlib.o hello.o hello && hello

.globl putc
.globl puts
.globl exit
.text

# putc(ch in a0): write the low byte of a0 to stdout (fd 1).
putc:
  addi sp, sp, -16
  sd a0, 0(sp)
  li a0, 1           # fd = stdout
  addi a1, sp, 0     # &byte
  li a2, 1           # len = 1
  li a7, 64          # write
  ecall
  addi sp, sp, 16
  ret

# puts(ptr in a0): write a NUL-terminated string to stdout, one byte via putc.
puts:
  addi sp, sp, -16
  sd ra, 8(sp)
  sd s0, 0(sp)
  mv s0, a0          # s0 = cursor
puts_loop:
  lbu a0, 0(s0)      # ch = *cursor
  beq a0, x0, puts_done
  call putc          # local call: resolved within this object, no relocation
  addi s0, s0, 1
  j puts_loop
puts_done:
  ld s0, 0(sp)
  ld ra, 8(sp)
  addi sp, sp, 16
  ret

# exit(code in a0): terminate the process with the given status.
exit:
  li a7, 93          # exit
  ecall
