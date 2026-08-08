# 05 template: write sos(a, b) = square(a) + square(b).

.globl _start
_start:
	li   a0, 3           # a
	li   a1, 4           # b
	jal  ra, sos         # a0 should end up = sos(3, 4) = 25 (=0x19)
done:
	j    done

square:                # square(x): return x * x (a leaf, so it needs no frame)
	mul  a0, a0, a0
	ret

# sos(a, b): square(a) + square(b). Each call to square is free to clobber a0
# and a1, so whatever you still need afterwards has to live in the frame.
#
# Frame plan (32 bytes, fp = the sp on entry):
#   -8(fp)   saved ra
#   -16(fp)  saved fp
#   -24(fp)  square(a)   (kept across the second call)
#   -32(fp)  b           (kept across the first call)
sos:
	# prologue: reserve the frame, save fp then ra, and point fp at the base
	# TODO

	# save b, run square(a), then save its result
	# TODO

	# reload b, run square(b), then add square(a) back in
	# TODO

	# epilogue: restore ra and fp, give the frame back, ret
	ret
