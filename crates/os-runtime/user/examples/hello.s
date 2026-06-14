# What cc must emit for hello.hll: naive stack-machine RISC-V.
# Prints "HLL0\nY", exits with sum_to(8) = 36.

.text
.globl _start
_start:
  # --- main() ---
  addi sp, sp, -16
  sd ra, 8(sp)
  li a0, 72          # putc('H')
  call putc
  li a0, 76          # putc('L')
  call putc
  li a0, 76          # putc('L')
  call putc
  li a0, 48          # putc('0')
  call putc
  li a0, 10          # putc('\n')
  call putc
  li a0, 8
  call sum_to        # a0 = sum_to(8) = 36
  sd a0, 0(sp)       # n = 36
  # if n > 30 { putc('Y') }
  ld t0, 0(sp)
  li t1, 30
  bge t1, t0, skip   # skip when 30 >= n
  li a0, 89          # putc('Y')
  call putc
skip:
  ld a0, 0(sp)       # return value = n
  ld ra, 8(sp)
  addi sp, sp, 16
  li a7, 93          # exit(n)
  ecall

# putc(ch in a0): write the low byte of a0 to fd 1 (the UART).
putc:
  addi sp, sp, -16
  sd a0, 0(sp)       # byte to write (low byte at sp+0)
  li a0, 1           # fd = stdout
  addi a1, sp, 0     # &byte
  li a2, 1           # len = 1
  li a7, 64          # write
  ecall
  addi sp, sp, 16
  ret

# sum_to(n in a0): return 1 + 2 + ... + n.
sum_to:
  addi sp, sp, -32
  sd ra, 24(sp)
  sd a0, 16(sp)      # n
  li t0, 0
  sd t0, 8(sp)       # total = 0
  li t0, 1
  sd t0, 0(sp)       # i = 1
loop:
  ld t0, 0(sp)       # i
  ld t1, 16(sp)      # n
  blt t1, t0, done   # stop once n < i (i > n)
  ld t2, 8(sp)       # total
  add t2, t2, t0     # total += i
  sd t2, 8(sp)
  addi t0, t0, 1     # i += 1
  sd t0, 0(sp)
  j loop
done:
  ld a0, 8(sp)       # return total
  ld ra, 24(sp)
  addi sp, sp, 32
  ret
