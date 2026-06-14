# Compute the 11th Fibonacci number iteratively and exit with it (89).
# Assemble and run from the shell:
#   as /home/fib.s /home/fib.elf
#   run /home/fib.elf
# Uses only the in-VM assembler subset: li, add, mv, addi, beq, j, ecall.

  li t0, 0        # a = fib(0)
  li t1, 1        # b = fib(1)
  li t2, 0        # i = 0
  li t3, 11       # n
loop:
  beq t2, t3, done
  add t4, t0, t1  # t = a + b
  mv t0, t1       # a = b
  mv t1, t4       # b = t
  addi t2, t2, 1  # i++
  j loop
done:
  mv a0, t0       # exit code = fib(n)
  li a7, 93       # exit syscall
  ecall
