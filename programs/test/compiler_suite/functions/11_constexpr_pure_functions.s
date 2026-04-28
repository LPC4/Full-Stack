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
	addi   t0, sp, 4
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; if condition
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 1
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 12(sp)
	lb     t4, 12(sp)
	bne t4, zero, factorial__label_0
	j factorial__label_2
factorial__label_0:
	addi   t5, zero, 1
	addi   a0, t5, 0
	ld     s0, 48(sp)
	ld     ra, 40(sp)
	addi   sp, sp, 64
	jalr   zero, 0(ra)
factorial__label_2:
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	lw     t0, 20(sp)
	addi   t1, zero, 1
	sub    t2, t0, t1
	sw     t2, 24(sp)
	lw     t0, 24(sp)
	addi   a0, t0, 0
	jal ra, factorial
	sd     a0, 28(sp)
	lw     t0, 16(sp)
	lw     t1, 28(sp)
	mul    t2, t0, t1
	sw     t2, 32(sp)
	lw     t3, 32(sp)
	addi   a0, t3, 0
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
	addi   t4, s0, 80
	sw     a0, 0(sp)
fibonacci__entry:
	; bind parameter: n
	addi   t0, sp, 4
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; if condition
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 0
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 12(sp)
	lb     t4, 12(sp)
	bne t4, zero, fibonacci__label_3
	j fibonacci__label_5
fibonacci__label_3:
	addi   t5, zero, 0
	addi   a0, t5, 0
	ld     s0, 64(sp)
	ld     ra, 56(sp)
	addi   sp, sp, 80
	jalr   zero, 0(ra)
fibonacci__label_5:
	; if condition
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	lw     t0, 16(sp)
	addi   t1, zero, 1
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 20(sp)
	lb     t4, 20(sp)
	bne t4, zero, fibonacci__label_6
	j fibonacci__label_8
fibonacci__label_6:
	addi   t5, zero, 1
	addi   a0, t5, 0
	ld     s0, 64(sp)
	ld     ra, 56(sp)
	addi   sp, sp, 80
	jalr   zero, 0(ra)
fibonacci__label_8:
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	lw     t0, 24(sp)
	addi   t1, zero, 1
	sub    t2, t0, t1
	sw     t2, 28(sp)
	lw     t0, 28(sp)
	addi   a0, t0, 0
	jal ra, fibonacci
	sd     a0, 32(sp)
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	lw     t0, 36(sp)
	addi   t1, zero, 2
	sub    t2, t0, t1
	sw     t2, 40(sp)
	lw     t0, 40(sp)
	addi   a0, t0, 0
	jal ra, fibonacci
	sd     a0, 44(sp)
	lw     t0, 32(sp)
	lw     t1, 44(sp)
	add    t2, t0, t1
	sw     t2, 48(sp)
	lw     t3, 48(sp)
	addi   a0, t3, 0
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
	addi   t4, s0, 64
	sw     a0, 0(sp)
	sw     a1, 4(sp)
	sw     a2, 8(sp)
add_multiply__entry:
	; bind parameter: a
	addi   t0, sp, 12
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; bind parameter: b
	addi   t0, sp, 16
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	; bind parameter: c
	addi   t0, sp, 20
	lw     t1, 8(sp)
	sw     t1, 0(t0)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	lw     t0, 24(sp)
	lw     t1, 28(sp)
	add    t2, t0, t1
	sw     t2, 32(sp)
	addi   t0, sp, 20
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	lw     t0, 32(sp)
	lw     t1, 36(sp)
	mul    t2, t0, t1
	sw     t2, 40(sp)
	lw     t3, 40(sp)
	addi   a0, t3, 0
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
	addi   t4, s0, 64
	sw     a0, 0(sp)
	sw     a1, 4(sp)
max_value__entry:
	; bind parameter: a
	addi   t0, sp, 8
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; bind parameter: b
	addi   t0, sp, 12
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	; if condition
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	lw     t0, 16(sp)
	lw     t1, 20(sp)
	slt    t2, t1, t0
	sb     t2, 24(sp)
	lb     t3, 24(sp)
	bne t3, zero, max_value__label_9
	j max_value__label_11
max_value__label_9:
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	lw     t2, 28(sp)
	addi   a0, t2, 0
	ld     s0, 48(sp)
	ld     ra, 40(sp)
	addi   sp, sp, 64
	jalr   zero, 0(ra)
max_value__label_11:
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t2, 32(sp)
	addi   a0, t2, 0
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
	addi   t0, zero, 120
	addi   a0, t0, 0
	jal ra, print
	lui    t0, 0x376
	addi   t0, t0, -256
	addi   a0, t0, 0
	jal ra, print
	addi   t0, zero, 55
	addi   a0, t0, 0
	jal ra, print
	addi   t0, zero, 20
	addi   a0, t0, 0
	jal ra, print
	addi   t0, zero, 42
	addi   a0, t0, 0
	jal ra, print
	lui    t0, 0xa
	addi   t0, t0, -640
	addi   a0, t0, 0
	jal ra, print
	ld     s0, 8(sp)
	ld     ra, 0(sp)
	addi   sp, sp, 16
	jalr   zero, 0(ra)