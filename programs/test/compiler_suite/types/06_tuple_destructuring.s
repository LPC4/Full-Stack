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
	addi   t0, sp, 8
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; bind parameter: b
	addi   t0, sp, 12
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	lw     t0, 16(sp)
	lw     t1, 20(sp)
	div    t2, t0, t1
	sw     t2, 24(sp)
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t0, 28(sp)
	lw     t1, 32(sp)
	rem    t2, t0, t1
	sw     t2, 36(sp)
	addi   t0, sp, 40
	lw     t1, 24(sp)
	sw     t1, 0(t0)
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
	addi   t0, zero, 10
	addi   a0, t0, 0
	addi   t1, zero, 3
	addi   a1, t1, 0
	jal ra, divide
	sw     a0, 0(sp)
	sw     a1, 4(sp)
	ld     t0, 0(sp)
	addi   t1, t0, 0
	lw     t2, 0(t1)
	sw     t2, 8(sp)
	addi   t0, sp, 12
	lw     t1, 8(sp)
	sw     t1, 0(t0)
	ld     t0, 0(sp)
	addi   t1, t0, 4
	lw     t2, 0(t1)
	sw     t2, 16(sp)
	addi   t0, sp, 20
	lw     t1, 16(sp)
	sw     t1, 0(t0)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	lw     t2, 24(sp)
	addi   a0, t2, 0
	ld     s0, 40(sp)
	ld     ra, 32(sp)
	addi   sp, sp, 48
	jalr   zero, 0(ra)