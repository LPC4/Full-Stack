# Sum the integers 1..10 and exit with the total (55).
# Assemble and run from the shell:
#   as /home/sum.s /home/sum.fexe
#   run /home/sum.fexe
# Uses only the in-VM assembler subset: li, add, addi, beq, j, ecall.

  li a0, 0        # total = 0
  li t0, 1        # i = 1
  li t1, 11       # limit (exclusive)
loop:
  beq t0, t1, done
  add a0, a0, t0  # total += i
  addi t0, t0, 1  # i++
  j loop
done:
  li a7, 93       # exit syscall
  ecall           # exit(total)
