# 05.2: recursive factorial. Each call keeps a frame pointer so it survives the nested call.

.globl _start
_start:
  li   a0, 5           # n
  jal  ra, fact        # a0 = fact(5) = 120
done:
  j    done

fact:                  # fact(n): n < 2 returns 1, else n * fact(n - 1)
  addi sp, sp, -32     # prologue: reserve a 32-byte frame
  sd   fp, 16(sp)      # save the caller's frame pointer before we overwrite fp
  addi fp, sp, 32      # fp = base of this frame: the old sp
  sd   ra, -8(fp)      # save the return address at -8(fp)
  li   t0, 2
  blt  a0, t0, base    # n < 2: fact(n) = 1, so there is nothing to compute
  sd   a0, -24(fp)     # keep n in a local, so the call cannot lose it
  addi a0, a0, -1
  jal  ra, fact        # a0 = fact(n - 1)
  ld   t1, -24(fp)     # reload n
  mul  a0, a0, t1      # a0 = n * fact(n - 1)
  j    epilogue
base:
  li   a0, 1           # fact(n) = 1 for n < 2
epilogue:
  ld   ra, -8(fp)      # restore what we saved, in reverse
  ld   fp, -16(fp)
  addi sp, sp, 32
  ret
