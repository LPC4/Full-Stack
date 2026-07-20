# 04.2: a function that calls another function must save ra, because jal overwrites it.

.globl _start
_start:
  li   a0, 4
  li   a1, 9
  li   a2, 6
  jal  ra, max3       # a0 = max3(4, 9, 6)
done:
  j    done

max3:                 # max3(a, b, c): the largest of three, via two calls to max
  addi sp, sp, -16    # prologue: grow the stack by two 8-byte slots
  sd   ra, 8(sp)      # save our return address; without this, ret loops forever
  sd   s0, 0(sp)      # save s0 before using it: its value belongs to our caller
  mv   s0, a2         # move c into s0: a call may clobber any a or t register
  jal  ra, max        # a0 = max(a, b)
  mv   a1, s0         # bring c back as the second argument
  jal  ra, max        # a0 = max(max(a, b), c)
  ld   s0, 0(sp)      # epilogue: restore what we saved, in reverse
  ld   ra, 8(sp)
  addi sp, sp, 16     # give the stack space back
  ret

max:                  # max(a, b), unchanged from part 1
  bge  a0, a1, max_a
  mv   a0, a1
max_a:
  ret
