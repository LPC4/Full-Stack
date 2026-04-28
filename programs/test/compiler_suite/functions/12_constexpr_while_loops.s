.section .text
.globl sum_to_n
sum_to_n:
	addi   sp, sp, -80
	sd     ra, 56(sp)
	sd     s0, 64(sp)
	addi   s0, sp, 0
	addi   t0, s0, 80
	sw     a0, 0(sp)
sum_to_n__entry:
	; bind parameter: n
	addi   t0, sp, 4
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; local var: result
	addi   t0, sp, 8
	addi   t1, zero, 0
	sw     t1, 0(t0)
	; local var: i
	addi   t0, sp, 12
	addi   t1, zero, 1
	sw     t1, 0(t0)
	j sum_to_n__label_0
sum_to_n__label_0:
	; while condition
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	lw     t0, 16(sp)
	lw     t1, 20(sp)
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 24(sp)
	lb     t4, 24(sp)
	bne t4, zero, sum_to_n__label_1
	j sum_to_n__label_2
sum_to_n__label_1:
	; assignment
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t0, 28(sp)
	lw     t1, 32(sp)
	add    t2, t0, t1
	sw     t2, 36(sp)
	addi   t0, sp, 8
	lw     t1, 36(sp)
	sw     t1, 0(t0)
	; assignment
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	lw     t0, 40(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 44(sp)
	addi   t0, sp, 12
	lw     t1, 44(sp)
	sw     t1, 0(t0)
	j sum_to_n__label_0
sum_to_n__label_2:
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 48(sp)
	lw     t2, 48(sp)
	addi   a0, t2, 0
	ld     s0, 64(sp)
	ld     ra, 56(sp)
	addi   sp, sp, 80
	jalr   zero, 0(ra)
.globl factorial_while
factorial_while:
	addi   sp, sp, -80
	sd     ra, 56(sp)
	sd     s0, 64(sp)
	addi   s0, sp, 0
	addi   t3, s0, 80
	sw     a0, 0(sp)
factorial_while__entry:
	; bind parameter: n
	addi   t0, sp, 4
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; local var: result
	addi   t0, sp, 8
	addi   t1, zero, 1
	sw     t1, 0(t0)
	; local var: i
	addi   t0, sp, 12
	addi   t1, zero, 2
	sw     t1, 0(t0)
	j factorial_while__label_3
factorial_while__label_3:
	; while condition
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	lw     t0, 16(sp)
	lw     t1, 20(sp)
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 24(sp)
	lb     t4, 24(sp)
	bne t4, zero, factorial_while__label_4
	j factorial_while__label_5
factorial_while__label_4:
	; assignment
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t0, 28(sp)
	lw     t1, 32(sp)
	mul    t2, t0, t1
	sw     t2, 36(sp)
	addi   t0, sp, 8
	lw     t1, 36(sp)
	sw     t1, 0(t0)
	; assignment
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	lw     t0, 40(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 44(sp)
	addi   t0, sp, 12
	lw     t1, 44(sp)
	sw     t1, 0(t0)
	j factorial_while__label_3
factorial_while__label_5:
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 48(sp)
	lw     t2, 48(sp)
	addi   a0, t2, 0
	ld     s0, 64(sp)
	ld     ra, 56(sp)
	addi   sp, sp, 80
	jalr   zero, 0(ra)
.globl main
main:
	addi   sp, sp, -16
	sd     ra, 0(sp)
	sd     s0, 8(sp)
	addi   s0, sp, 0
main__entry:
	addi   t0, zero, 55
	addi   a0, t0, 0
	jal ra, print
	lui    t0, 0x1
	addi   t0, t0, 944
	addi   a0, t0, 0
	jal ra, print
	ld     s0, 8(sp)
	ld     ra, 0(sp)
	addi   sp, sp, 16
	jalr   zero, 0(ra)