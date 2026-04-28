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
	addi   t0, sp, 4
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; local var: sum
	addi   t0, sp, 8
	addi   t1, zero, 0
	sw     t1, 0(t0)
	j control_suite__label_0
control_suite__label_0:
	; while condition
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 12(sp)
	lw     t0, 12(sp)
	addi   t1, zero, 0
	slt    t2, t1, t0
	sb     t2, 16(sp)
	lb     t3, 16(sp)
	bne t3, zero, control_suite__label_1
	j control_suite__label_2
control_suite__label_1:
	; if condition
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	lw     t0, 20(sp)
	addi   t1, zero, 5
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 24(sp)
	lb     t4, 24(sp)
	bne t4, zero, control_suite__label_3
	j control_suite__label_4
control_suite__label_3:
	; assignment
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	lw     t0, 28(sp)
	addi   t1, zero, 10
	add    t2, t0, t1
	sw     t2, 32(sp)
	addi   t0, sp, 8
	lw     t1, 32(sp)
	sw     t1, 0(t0)
	j control_suite__label_5
control_suite__label_4:
	; assignment
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	lw     t0, 36(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 40(sp)
	addi   t0, sp, 8
	lw     t1, 40(sp)
	sw     t1, 0(t0)
	j control_suite__label_5
control_suite__label_5:
	; assignment
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 44(sp)
	lw     t0, 44(sp)
	addi   t1, zero, 1
	sub    t2, t0, t1
	sw     t2, 48(sp)
	addi   t0, sp, 4
	lw     t1, 48(sp)
	sw     t1, 0(t0)
	j control_suite__label_0
control_suite__label_2:
	; if condition
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 52(sp)
	lw     t0, 52(sp)
	addi   t1, zero, 0
	slt    t2, t1, t0
	sb     t2, 56(sp)
	lb     t3, 56(sp)
	bne t3, zero, control_suite__label_6
	j control_suite__label_8
control_suite__label_6:
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 60(sp)
	lw     t2, 60(sp)
	addi   a0, t2, 0
	ld     s0, 72(sp)
	ld     ra, 64(sp)
	addi   sp, sp, 80
	jalr   zero, 0(ra)
control_suite__label_8:
	addi   t3, zero, 0
	addi   a0, t3, 0
	ld     s0, 72(sp)
	ld     ra, 64(sp)
	addi   sp, sp, 80
	jalr   zero, 0(ra)