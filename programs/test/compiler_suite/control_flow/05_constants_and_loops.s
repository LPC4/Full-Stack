.section .text
.globl main
main:
	addi   sp, sp, -64
	sd     ra, 48(sp)
	sd     s0, 56(sp)
	addi   s0, sp, 0
main__entry:
	; local var: c
	addi   t0, zero, 10
	addi   t1, zero, 20
	add    t2, t0, t1
	sw     t2, 4(sp)
	addi   t0, sp, 0
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	j main__label_0
main__label_0:
	; while condition
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 50
	slt    t2, t0, t1
	sb     t2, 12(sp)
	lb     t3, 12(sp)
	bne t3, zero, main__label_1
	j main__label_2
main__label_1:
	; assignment
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	lw     t0, 16(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 20(sp)
	addi   t0, sp, 0
	lw     t1, 20(sp)
	sw     t1, 0(t0)
	; if condition
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	lw     t0, 24(sp)
	addi   t1, zero, 40
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 28(sp)
	lb     t4, 28(sp)
	bne t4, zero, main__label_3
	j main__label_5
main__label_3:
	j main__label_2
main__label_5:
	; if condition
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t0, 32(sp)
	addi   t1, zero, 35
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 36(sp)
	lb     t4, 36(sp)
	bne t4, zero, main__label_6
	j main__label_8
main__label_6:
	j main__label_0
main__label_8:
	j main__label_0
main__label_2:
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	lw     t2, 40(sp)
	addi   a0, t2, 0
	ld     s0, 56(sp)
	ld     ra, 48(sp)
	addi   sp, sp, 64
	jalr   zero, 0(ra)