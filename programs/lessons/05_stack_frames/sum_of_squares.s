# 05 solution: sos(a, b) = square(a) + square(b).

.globl _start
_start:
  li   a0, 3           # a
  li   a1, 4           # b
  jal  ra, sos         # a0 = sos(3, 4) = 25
done:
  j    done

square:                # square(x): return x * x (a leaf, so it needs no frame)
  mul  a0, a0, a0
  ret

sos:                   # sos(a, b): square(a) + square(b)
  addi sp, sp, -32     # prologue: reserve a 32-byte frame
  sd   fp, 16(sp)      # save the caller's frame pointer before we overwrite fp
  addi fp, sp, 32      # fp = base of this frame: the old sp
  sd   ra, -8(fp)      # save the return address at -8(fp)
  sd   a1, -32(fp)     # keep b: square(a) is about to clobber a1
  jal  ra, square      # a0 = square(a)
  sd   a0, -24(fp)     # keep square(a): square(b) is about to clobber a0
  ld   a0, -32(fp)     # a0 = b
  jal  ra, square      # a0 = square(b)
  ld   t1, -24(fp)     # t1 = square(a)
  add  a0, a0, t1      # square(a) + square(b)
  ld   ra, -8(fp)      # epilogue: restore what we saved, in reverse
  ld   fp, -16(fp)
  addi sp, sp, 32
  ret
