.section .text
.globl main
main:
	addi   sp, sp, -96
	sd     ra, 72(sp)
	sd     s0, 80(sp)
	addi   s0, sp, 0
main__entry:
	; local var: a
	addi   t0, sp, 0
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: b
	addi   t0, sp, 4
	addi   t1, zero, 20
	sw     t1, 0(t0)
	; local var: c
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 12(sp)
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	lw     t0, 12(sp)
	lw     t1, 16(sp)
	add    t2, t0, t1
	sw     t2, 20(sp)
	lw     t0, 20(sp)
	addi   t1, zero, 2
	mul    t2, t0, t1
	sw     t2, 24(sp)
	addi   t0, sp, 8
	lw     t1, 24(sp)
	sw     t1, 0(t0)
	; local var: d
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t0, 32(sp)
	addi   t1, zero, 5
	div    t2, t0, t1
	sw     t2, 36(sp)
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 44(sp)
	lw     t0, 40(sp)
	lw     t1, 44(sp)
	rem    t2, t0, t1
	sw     t2, 48(sp)
	lw     t0, 36(sp)
	lw     t1, 48(sp)
	sub    t2, t0, t1
	sw     t2, 52(sp)
	addi   t0, sp, 28
	lw     t1, 52(sp)
	sw     t1, 0(t0)
	; local var: e
	addi   t0, sp, 28
	lw     t1, 0(t0)
	sw     t1, 60(sp)
	lw     t0, 60(sp)
	sub    t1, zero, t0
	sw     t1, 64(sp)
	addi   t0, sp, 56
	lw     t1, 64(sp)
	sw     t1, 0(t0)
	addi   t0, sp, 56
	lw     t1, 0(t0)
	sw     t1, 68(sp)
	lw     t2, 68(sp)
	addi   a0, t2, 0
	ld     s0, 80(sp)
	ld     ra, 72(sp)
	addi   sp, sp, 96
	jalr   zero, 0(ra)