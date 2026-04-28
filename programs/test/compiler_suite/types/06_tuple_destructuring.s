.section .text
.globl divide
divide:
	addi   sp, sp, -64
	sd     ra, 48(sp)
	sd     s0, 56(sp)
	addi   s0, sp, 0
	addi   t0, s0, 64
	sw     a0, 0(sp)
	sw     a1, 4(sp)
divide__entry:
	; bind parameter: a
	addi   t1, sp, 8
	lw     t2, 0(sp)
	sw     t2, 0(t1)
	; bind parameter: b
	addi   t3, sp, 12
	lw     t4, 4(sp)
	sw     t4, 0(t3)
	addi   t5, sp, 8
	lw     t6, 0(t5)
	sw     t6, 16(sp)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	lw     t2, 16(sp)
	lw     t3, 20(sp)
	div    t4, t2, t3
	sd     t4, 24(sp)
	addi   t5, sp, 8
	lw     t6, 0(t5)
	sw     t6, 28(sp)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t2, 28(sp)
	lw     t3, 32(sp)
	rem    t4, t2, t3
	sd     t4, 36(sp)
	addi   t5, sp, 40
	lw     t6, 24(sp)
	sw     t6, 0(t5)
	addi   t0, sp, 44
	lw     t1, 36(sp)
	sw     t1, 0(t0)
	ld     a0, 40(sp)
	ld     s0, 56(sp)
	ld     ra, 48(sp)
	addi   sp, sp, 64
	jalr   zero, 0(ra)
.globl test_tuple_destructuring
test_tuple_destructuring:
	addi   sp, sp, -48
	sd     ra, 32(sp)
	sd     s0, 40(sp)
	addi   s0, sp, 0
test_tuple_destructuring__entry:
	; assignment
	addi   t2, zero, 10
	addi   a0, t2, 0
	addi   t3, zero, 3
	addi   a1, t3, 0
	jal ra, divide
	sd     a0, 0(sp)
	ld     t4, 0(sp)
	addi   t5, t4, 0
	lw     t6, 0(t5)
	sw     t6, 8(sp)
	addi   t0, sp, 12
	lw     t1, 8(sp)
	sw     t1, 0(t0)
	ld     t2, 0(sp)
	addi   t3, t2, 4
	lw     t4, 0(t3)
	sw     t4, 16(sp)
	addi   t5, sp, 20
	lw     t6, 16(sp)
	sw     t6, 0(t5)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	lw     t2, 24(sp)
	addi   a0, t2, 0
	ld     s0, 40(sp)
	ld     ra, 32(sp)
	addi   sp, sp, 48
	jalr   zero, 0(ra)