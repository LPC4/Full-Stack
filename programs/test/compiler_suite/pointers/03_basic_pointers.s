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
	addi   t0, sp, 4
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; local var: ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 16(sp)
	addi   t0, sp, 8
	ld     t1, 16(sp)
	sd     t1, 0(t0)
	; assignment
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 32(sp)
	ld     t0, 32(sp)
	lw     t1, 24(sp)
	sw     t1, 0(t0)
	; local var: val_ref
	addi   t0, sp, 40
	addi   t1, sp, 4
	sd     t1, 0(t0)
	; assignment
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 48(sp)
	ld     t0, 48(sp)
	lw     t1, 0(t0)
	sw     t1, 56(sp)
	lw     t0, 56(sp)
	addi   t1, zero, 10
	add    t2, t0, t1
	sw     t2, 60(sp)
	addi   t0, sp, 40
	ld     t1, 0(t0)
	sd     t1, 64(sp)
	ld     t0, 64(sp)
	lw     t1, 60(sp)
	sw     t1, 0(t0)
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 72(sp)
	; defer: captured call free with 1 args
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 80(sp)
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