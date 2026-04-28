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
	sd     t2, 4(sp)
	addi   t3, sp, 0
	lw     t4, 4(sp)
	sw     t4, 0(t3)
	j main__label_0
main__label_0:
	; while condition
	addi   t5, sp, 0
	lw     t6, 0(t5)
	sw     t6, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 50
	slt    t2, t0, t1
	sd     t2, 12(sp)
	lb     t3, 12(sp)
	bne t3, zero, main__label_2
	j main__label_1
main__label_1:
	; assignment
	addi   t4, sp, 0
	lw     t5, 0(t4)
	sw     t5, 16(sp)
	lw     t6, 16(sp)
	addi   t0, zero, 1
	add    t1, t6, t0
	sd     t1, 20(sp)
	addi   t2, sp, 0
	lw     t3, 20(sp)
	sw     t3, 0(t2)
	; if condition
	addi   t4, sp, 0
	lw     t5, 0(t4)
	sw     t5, 24(sp)
	lw     t6, 24(sp)
	addi   t0, zero, 40
	sub    t2, t6, t0
	sltiu  t1, t2, 1
	sd     t1, 28(sp)
	lb     t3, 28(sp)
	bne t3, zero, main__label_5
	j main__label_3
main__label_3:
	j main__label_2
main__label_5:
	; if condition
	addi   t4, sp, 0
	lw     t5, 0(t4)
	sw     t5, 32(sp)
	lw     t6, 32(sp)
	addi   t0, zero, 35
	sub    t2, t6, t0
	sltiu  t1, t2, 1
	sd     t1, 36(sp)
	lb     t3, 36(sp)
	bne t3, zero, main__label_8
	j main__label_6
main__label_6:
	j main__label_0
main__label_8:
	j main__label_0
main__label_2:
	addi   t4, sp, 0
	lw     t5, 0(t4)
	sw     t5, 40(sp)
	lw     t6, 40(sp)
	addi   a0, t6, 0
	ld     s0, 56(sp)
	ld     ra, 48(sp)
	addi   sp, sp, 64
	jalr   zero, 0(ra)