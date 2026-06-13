# Sum a five-element array built on the stack and exit with the total (42).
# Showcases the expanded in-VM assembler subset (PLAN 1.1): a stack frame,
# sd/ld with offset(reg) addressing, slli index scaling, and a bge-controlled
# loop. Assemble and run from the shell:
#   as /home/src/array.s /home/array.fexe
#   run /home/array.fexe          # exits with 42

  addi sp, sp, -64        # reserve five i64 slots (16-byte aligned)
  li t0, 3
  sd t0, 0(sp)            # arr[0] = 3
  li t0, 9
  sd t0, 8(sp)            # arr[1] = 9
  li t0, 15
  sd t0, 16(sp)           # arr[2] = 15
  li t0, 6
  sd t0, 24(sp)           # arr[3] = 6
  li t0, 9
  sd t0, 32(sp)           # arr[4] = 9

  li a0, 0                # running sum
  li t1, 0                # index i
  li t2, 5                # element count
loop:
  bge t1, t2, done        # stop once i reaches count
  slli t3, t1, 3          # byte offset = i * 8
  add  t4, sp, t3         # &arr[i]
  ld   t5, 0(t4)          # load arr[i]
  add  a0, a0, t5         # sum += arr[i]
  addi t1, t1, 1          # i++
  j loop
done:
  addi sp, sp, 64         # restore the stack pointer
  li a7, 93               # exit syscall
  ecall
