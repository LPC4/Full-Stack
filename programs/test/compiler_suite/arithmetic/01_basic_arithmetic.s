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
	addi   t2, sp, 4
	addi   t3, zero, 20
	sw     t3, 0(t2)
	; local var: c
	addi   t4, sp, 0
	lw     t5, 0(t4)
	sw     t5, 12(sp)
	addi   t6, sp, 4
	lw     t0, 0(t6)
	sw     t0, 16(sp)
	lw     t1, 12(sp)
	lw     t2, 16(sp)
	add    t3, t1, t2
	sd     t3, 20(sp)
	lw     t4, 20(sp)
	addi   t5, zero, 2
	mul    t6, t4, t5
	sd     t6, 24(sp)
	addi   t0, sp, 8
	lw     t1, 24(sp)
	sw     t1, 0(t0)
	; local var: d
	addi   t2, sp, 8
	lw     t3, 0(t2)
	sw     t3, 32(sp)
	lw     t4, 32(sp)
	addi   t5, zero, 5
	div    t6, t4, t5
	sd     t6, 36(sp)
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	addi   t2, sp, 4
	lw     t3, 0(t2)
	sw     t3, 44(sp)
	lw     t4, 40(sp)
	lw     t5, 44(sp)
	rem    t6, t4, t5
	sd     t6, 48(sp)
	lw     t0, 36(sp)
	lw     t1, 48(sp)
	sub    t2, t0, t1
	sd     t2, 52(sp)
	addi   t3, sp, 28
	lw     t4, 52(sp)
	sw     t4, 0(t3)
	; local var: e
	addi   t5, sp, 28
	lw     t6, 0(t5)
	sw     t6, 60(sp)
	lw     t0, 60(sp)
	sub    t1, zero, t0
	sd     t1, 64(sp)
	addi   t2, sp, 56
	lw     t3, 64(sp)
	sw     t3, 0(t2)
	addi   t4, sp, 56
	lw     t5, 0(t4)
	sw     t5, 68(sp)
	lw     t6, 68(sp)
	addi   a0, t6, 0
	ld     s0, 80(sp)
	ld     ra, 72(sp)
	addi   sp, sp, 96
	jalr   zero, 0(ra)