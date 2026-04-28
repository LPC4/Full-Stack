.section .text
.globl test_simple
test_simple:
	addi   sp, sp, -32
	sd     ra, 16(sp)
	sd     s0, 24(sp)
	addi   s0, sp, 0
test_simple__entry:
	; local var: q
	addi   t0, sp, 0
	addi   t1, zero, 0
	sw     t1, 0(t0)
	; local var: r
	addi   t0, sp, 4
	addi   t1, zero, 0
	sw     t1, 0(t0)
	; assignment
	addi   t0, sp, 0
	addi   t1, zero, 5
	sw     t1, 0(t0)
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t2, 8(sp)
	addi   a0, t2, 0
	ld     s0, 24(sp)
	ld     ra, 16(sp)
	addi   sp, sp, 32
	jalr   zero, 0(ra)