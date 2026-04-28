.section .text
.globl control_suite
control_suite:
	addi   sp, sp, -80
	sd     ra, 64(sp)
	sd     s0, 72(sp)
	addi   s0, sp, 0
	addi   t0, s0, 80
	sw     a0, 0(sp)
control_suite__entry:
	; bind parameter: param
	addi   t1, sp, 4
	lw     t2, 0(sp)
	sw     t2, 0(t1)
	; local var: sum
	addi   t3, sp, 8
	addi   t4, zero, 0
	sw     t4, 0(t3)
	j control_suite__label_0
control_suite__label_0:
	; while condition
	addi   t5, sp, 4
	lw     t6, 0(t5)
	sw     t6, 12(sp)
	lw     t0, 12(sp)
	addi   t1, zero, 0
	slt    t2, t1, t0
	sd     t2, 16(sp)
	lb     t3, 16(sp)
	bne t3, zero, control_suite__label_2
	j control_suite__label_1
control_suite__label_1:
	; if condition
	addi   t4, sp, 4
	lw     t5, 0(t4)
	sw     t5, 20(sp)
	lw     t6, 20(sp)
	addi   t0, zero, 5
	sub    t2, t6, t0
	sltiu  t1, t2, 1
	sd     t1, 24(sp)
	lb     t3, 24(sp)
	bne t3, zero, control_suite__label_4
	j control_suite__label_3
control_suite__label_3:
	; assignment
	addi   t4, sp, 8
	lw     t5, 0(t4)
	sw     t5, 28(sp)
	lw     t6, 28(sp)
	addi   t0, zero, 10
	add    t1, t6, t0
	sd     t1, 32(sp)
	addi   t2, sp, 8
	lw     t3, 32(sp)
	sw     t3, 0(t2)
	j control_suite__label_5
control_suite__label_4:
	; assignment
	addi   t4, sp, 8
	lw     t5, 0(t4)
	sw     t5, 36(sp)
	lw     t6, 36(sp)
	addi   t0, zero, 1
	add    t1, t6, t0
	sd     t1, 40(sp)
	addi   t2, sp, 8
	lw     t3, 40(sp)
	sw     t3, 0(t2)
	j control_suite__label_5
control_suite__label_5:
	; assignment
	addi   t4, sp, 4
	lw     t5, 0(t4)
	sw     t5, 44(sp)
	lw     t6, 44(sp)
	addi   t0, zero, 1
	sub    t1, t6, t0
	sd     t1, 48(sp)
	addi   t2, sp, 4
	lw     t3, 48(sp)
	sw     t3, 0(t2)
	j control_suite__label_0
control_suite__label_2:
	; if condition
	addi   t4, sp, 8
	lw     t5, 0(t4)
	sw     t5, 52(sp)
	lw     t6, 52(sp)
	addi   t0, zero, 0
	slt    t1, t0, t6
	sd     t1, 56(sp)
	lb     t2, 56(sp)
	bne t2, zero, control_suite__label_8
	j control_suite__label_6
control_suite__label_6:
	addi   t3, sp, 8
	lw     t4, 0(t3)
	sw     t4, 60(sp)
	lw     t5, 60(sp)
	addi   a0, t5, 0
control_suite__label_8:
	addi   t6, zero, 0
	addi   a0, t6, 0
	ld     s0, 72(sp)
	ld     ra, 64(sp)
	addi   sp, sp, 80
	jalr   zero, 0(ra)