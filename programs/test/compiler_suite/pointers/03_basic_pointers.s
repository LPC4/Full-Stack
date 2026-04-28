.section .text
.globl pointers
pointers:
	addi   sp, sp, -112
	sd     ra, 88(sp)
	sd     s0, 96(sp)
	addi   s0, sp, 0
	addi   t0, s0, 112
	sw     a0, 0(sp)
pointers__entry:
	; bind parameter: val
	addi   t1, sp, 4
	lw     t2, 0(sp)
	sw     t2, 0(t1)
	; local var: ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 16(sp)
	addi   t3, sp, 8
	ld     t4, 16(sp)
	sd     t4, 0(t3)
	; assignment
	addi   t5, sp, 4
	lw     t6, 0(t5)
	sw     t6, 24(sp)
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 32(sp)
	ld     t2, 32(sp)
	lw     t3, 24(sp)
	sw     t3, 0(t2)
	; local var: val_ref
	addi   t4, sp, 40
	addi   t5, sp, 4
	sd     t5, 0(t4)
	; assignment
	addi   t6, sp, 8
	ld     t0, 0(t6)
	sd     t0, 48(sp)
	ld     t1, 48(sp)
	lw     t2, 0(t1)
	sw     t2, 56(sp)
	lw     t3, 56(sp)
	addi   t4, zero, 10
	add    t5, t3, t4
	sd     t5, 60(sp)
	addi   t6, sp, 40
	ld     t0, 0(t6)
	sd     t0, 64(sp)
	ld     t1, 64(sp)
	lw     t2, 60(sp)
	sw     t2, 0(t1)
	addi   t3, sp, 8
	ld     t4, 0(t3)
	sd     t4, 72(sp)
	; defer: captured call free with 1 args
	addi   t5, sp, 4
	lw     t6, 0(t5)
	sw     t6, 80(sp)
	; executing deferred cleanup before return
	ld     t0, 72(sp)
	addi   a0, t0, 0
	jal ra, free
	lw     t1, 80(sp)
	addi   a0, t1, 0
	ld     s0, 96(sp)
	ld     ra, 88(sp)
	addi   sp, sp, 112
	jalr   zero, 0(ra)