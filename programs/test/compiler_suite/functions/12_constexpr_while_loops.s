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
	addi   t1, sp, 4
	lw     t2, 0(sp)
	sw     t2, 0(t1)
	; local var: result
	addi   t3, sp, 8
	addi   t4, zero, 0
	sw     t4, 0(t3)
	; local var: i
	addi   t5, sp, 12
	addi   t6, zero, 1
	sw     t6, 0(t5)
	j sum_to_n__label_0
sum_to_n__label_0:
	; while condition
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	addi   t2, sp, 4
	lw     t3, 0(t2)
	sw     t3, 20(sp)
	lw     t4, 16(sp)
	lw     t5, 20(sp)
	slt    t0, t5, t4
	sltiu  t6, t0, 1
	sd     t6, 24(sp)
	lb     t1, 24(sp)
	bne t1, zero, sum_to_n__label_2
	j sum_to_n__label_1
sum_to_n__label_1:
	; assignment
	addi   t2, sp, 8
	lw     t3, 0(t2)
	sw     t3, 28(sp)
	addi   t4, sp, 12
	lw     t5, 0(t4)
	sw     t5, 32(sp)
	lw     t6, 28(sp)
	lw     t0, 32(sp)
	add    t1, t6, t0
	sd     t1, 36(sp)
	addi   t2, sp, 8
	lw     t3, 36(sp)
	sw     t3, 0(t2)
	; assignment
	addi   t4, sp, 12
	lw     t5, 0(t4)
	sw     t5, 40(sp)
	lw     t6, 40(sp)
	addi   t0, zero, 1
	add    t1, t6, t0
	sd     t1, 44(sp)
	addi   t2, sp, 12
	lw     t3, 44(sp)
	sw     t3, 0(t2)
	j sum_to_n__label_0
sum_to_n__label_2:
	addi   t4, sp, 8
	lw     t5, 0(t4)
	sw     t5, 48(sp)
	lw     t6, 48(sp)
	addi   a0, t6, 0
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
	addi   t0, s0, 80
	sw     a0, 0(sp)
factorial_while__entry:
	; bind parameter: n
	addi   t1, sp, 4
	lw     t2, 0(sp)
	sw     t2, 0(t1)
	; local var: result
	addi   t3, sp, 8
	addi   t4, zero, 1
	sw     t4, 0(t3)
	; local var: i
	addi   t5, sp, 12
	addi   t6, zero, 2
	sw     t6, 0(t5)
	j factorial_while__label_3
factorial_while__label_3:
	; while condition
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	addi   t2, sp, 4
	lw     t3, 0(t2)
	sw     t3, 20(sp)
	lw     t4, 16(sp)
	lw     t5, 20(sp)
	slt    t0, t5, t4
	sltiu  t6, t0, 1
	sd     t6, 24(sp)
	lb     t1, 24(sp)
	bne t1, zero, factorial_while__label_5
	j factorial_while__label_4
factorial_while__label_4:
	; assignment
	addi   t2, sp, 8
	lw     t3, 0(t2)
	sw     t3, 28(sp)
	addi   t4, sp, 12
	lw     t5, 0(t4)
	sw     t5, 32(sp)
	lw     t6, 28(sp)
	lw     t0, 32(sp)
	mul    t1, t6, t0
	sd     t1, 36(sp)
	addi   t2, sp, 8
	lw     t3, 36(sp)
	sw     t3, 0(t2)
	; assignment
	addi   t4, sp, 12
	lw     t5, 0(t4)
	sw     t5, 40(sp)
	lw     t6, 40(sp)
	addi   t0, zero, 1
	add    t1, t6, t0
	sd     t1, 44(sp)
	addi   t2, sp, 12
	lw     t3, 44(sp)
	sw     t3, 0(t2)
	j factorial_while__label_3
factorial_while__label_5:
	addi   t4, sp, 8
	lw     t5, 0(t4)
	sw     t5, 48(sp)
	lw     t6, 48(sp)
	addi   a0, t6, 0
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
	lui    t1, 0x1
	addi   t1, t1, 944
	addi   a0, t1, 0
	jal ra, print
	ld     s0, 8(sp)
	ld     ra, 0(sp)
	addi   sp, sp, 16
	jalr   zero, 0(ra)