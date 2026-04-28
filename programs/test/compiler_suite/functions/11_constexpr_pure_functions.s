.section .text
.globl factorial
factorial:
	addi   sp, sp, -64
	sd     ra, 40(sp)
	sd     s0, 48(sp)
	addi   s0, sp, 0
	addi   t0, s0, 64
	sw     a0, 0(sp)
factorial__entry:
	; bind parameter: n
	addi   t1, sp, 4
	lw     t2, 0(sp)
	sw     t2, 0(t1)
	; if condition
	addi   t3, sp, 4
	lw     t4, 0(t3)
	sw     t4, 8(sp)
	lw     t5, 8(sp)
	addi   t6, zero, 1
	slt    t1, t6, t5
	sltiu  t0, t1, 1
	sd     t0, 12(sp)
	lb     t2, 12(sp)
	bne t2, zero, factorial__label_2
	j factorial__label_0
factorial__label_0:
	addi   t3, zero, 1
	addi   a0, t3, 0
factorial__label_2:
	addi   t4, sp, 4
	lw     t5, 0(t4)
	sw     t5, 16(sp)
	addi   t6, sp, 4
	lw     t0, 0(t6)
	sw     t0, 20(sp)
	lw     t1, 20(sp)
	addi   t2, zero, 1
	sub    t3, t1, t2
	sd     t3, 24(sp)
	lw     t4, 24(sp)
	addi   a0, t4, 0
	jal ra, factorial
	sd     a0, 28(sp)
	lw     t5, 16(sp)
	lw     t6, 28(sp)
	mul    t0, t5, t6
	sd     t0, 32(sp)
	lw     t1, 32(sp)
	addi   a0, t1, 0
	ld     s0, 48(sp)
	ld     ra, 40(sp)
	addi   sp, sp, 64
	jalr   zero, 0(ra)
.globl fibonacci
fibonacci:
	addi   sp, sp, -80
	sd     ra, 56(sp)
	sd     s0, 64(sp)
	addi   s0, sp, 0
	addi   t2, s0, 80
	sw     a0, 0(sp)
fibonacci__entry:
	; bind parameter: n
	addi   t3, sp, 4
	lw     t4, 0(sp)
	sw     t4, 0(t3)
	; if condition
	addi   t5, sp, 4
	lw     t6, 0(t5)
	sw     t6, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 0
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sd     t2, 12(sp)
	lb     t4, 12(sp)
	bne t4, zero, fibonacci__label_5
	j fibonacci__label_3
fibonacci__label_3:
	addi   t5, zero, 0
	addi   a0, t5, 0
fibonacci__label_5:
	; if condition
	addi   t6, sp, 4
	lw     t0, 0(t6)
	sw     t0, 16(sp)
	lw     t1, 16(sp)
	addi   t2, zero, 1
	sub    t4, t1, t2
	sltiu  t3, t4, 1
	sd     t3, 20(sp)
	lb     t5, 20(sp)
	bne t5, zero, fibonacci__label_8
	j fibonacci__label_6
fibonacci__label_6:
	addi   t6, zero, 1
	addi   a0, t6, 0
fibonacci__label_8:
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	lw     t2, 24(sp)
	addi   t3, zero, 1
	sub    t4, t2, t3
	sd     t4, 28(sp)
	lw     t5, 28(sp)
	addi   a0, t5, 0
	jal ra, fibonacci
	sd     a0, 32(sp)
	addi   t6, sp, 4
	lw     t0, 0(t6)
	sw     t0, 36(sp)
	lw     t1, 36(sp)
	addi   t2, zero, 2
	sub    t3, t1, t2
	sd     t3, 40(sp)
	lw     t4, 40(sp)
	addi   a0, t4, 0
	jal ra, fibonacci
	sd     a0, 44(sp)
	lw     t5, 32(sp)
	lw     t6, 44(sp)
	add    t0, t5, t6
	sd     t0, 48(sp)
	lw     t1, 48(sp)
	addi   a0, t1, 0
	ld     s0, 64(sp)
	ld     ra, 56(sp)
	addi   sp, sp, 80
	jalr   zero, 0(ra)
.globl add_multiply
add_multiply:
	addi   sp, sp, -64
	sd     ra, 48(sp)
	sd     s0, 56(sp)
	addi   s0, sp, 0
	addi   t2, s0, 64
	sw     a0, 0(sp)
	sw     a1, 4(sp)
	sw     a2, 8(sp)
add_multiply__entry:
	; bind parameter: a
	addi   t3, sp, 12
	lw     t4, 0(sp)
	sw     t4, 0(t3)
	; bind parameter: b
	addi   t5, sp, 16
	lw     t6, 4(sp)
	sw     t6, 0(t5)
	; bind parameter: c
	addi   t0, sp, 20
	lw     t1, 8(sp)
	sw     t1, 0(t0)
	addi   t2, sp, 12
	lw     t3, 0(t2)
	sw     t3, 24(sp)
	addi   t4, sp, 16
	lw     t5, 0(t4)
	sw     t5, 28(sp)
	lw     t6, 24(sp)
	lw     t0, 28(sp)
	add    t1, t6, t0
	sd     t1, 32(sp)
	addi   t2, sp, 20
	lw     t3, 0(t2)
	sw     t3, 36(sp)
	lw     t4, 32(sp)
	lw     t5, 36(sp)
	mul    t6, t4, t5
	sd     t6, 40(sp)
	lw     t0, 40(sp)
	addi   a0, t0, 0
	ld     s0, 56(sp)
	ld     ra, 48(sp)
	addi   sp, sp, 64
	jalr   zero, 0(ra)
.globl max_value
max_value:
	addi   sp, sp, -64
	sd     ra, 40(sp)
	sd     s0, 48(sp)
	addi   s0, sp, 0
	addi   t1, s0, 64
	sw     a0, 0(sp)
	sw     a1, 4(sp)
max_value__entry:
	; bind parameter: a
	addi   t2, sp, 8
	lw     t3, 0(sp)
	sw     t3, 0(t2)
	; bind parameter: b
	addi   t4, sp, 12
	lw     t5, 4(sp)
	sw     t5, 0(t4)
	; if condition
	addi   t6, sp, 8
	lw     t0, 0(t6)
	sw     t0, 16(sp)
	addi   t1, sp, 12
	lw     t2, 0(t1)
	sw     t2, 20(sp)
	lw     t3, 16(sp)
	lw     t4, 20(sp)
	slt    t5, t4, t3
	sd     t5, 24(sp)
	lb     t6, 24(sp)
	bne t6, zero, max_value__label_11
	j max_value__label_9
max_value__label_9:
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	lw     t2, 28(sp)
	addi   a0, t2, 0
max_value__label_11:
	addi   t3, sp, 12
	lw     t4, 0(t3)
	sw     t4, 32(sp)
	lw     t5, 32(sp)
	addi   a0, t5, 0
	ld     s0, 48(sp)
	ld     ra, 40(sp)
	addi   sp, sp, 64
	jalr   zero, 0(ra)
.globl main
main:
	addi   sp, sp, -16
	sd     ra, 0(sp)
	sd     s0, 8(sp)
	addi   s0, sp, 0
main__entry:
	addi   t6, zero, 120
	addi   a0, t6, 0
	jal ra, print
	lui    t0, 0x376
	addi   t0, t0, -256
	addi   a0, t0, 0
	jal ra, print
	addi   t1, zero, 55
	addi   a0, t1, 0
	jal ra, print
	addi   t2, zero, 20
	addi   a0, t2, 0
	jal ra, print
	addi   t3, zero, 42
	addi   a0, t3, 0
	jal ra, print
	lui    t4, 0xa
	addi   t4, t4, -640
	addi   a0, t4, 0
	jal ra, print
	ld     s0, 8(sp)
	ld     ra, 0(sp)
	addi   sp, sp, 16
	jalr   zero, 0(ra)