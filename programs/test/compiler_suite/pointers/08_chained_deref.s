.section .text
.globl chained_deref_assign
chained_deref_assign:
	addi   sp, sp, -96
	sd     ra, 80(sp)
	sd     s0, 88(sp)
	addi   s0, sp, 0
	addi   t0, s0, 96
	sw     a0, 0(sp)
chained_deref_assign__entry:
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
	; local var: pp
	addi   t0, sp, 24
	addi   t1, sp, 8
	sd     t1, 0(t0)
	; assignment
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t0, 32(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 36(sp)
	addi   t0, sp, 24
	ld     t1, 0(t0)
	sd     t1, 40(sp)
	ld     t0, 40(sp)
	ld     t1, 0(t0)
	sd     t1, 48(sp)
	ld     t0, 48(sp)
	lw     t1, 36(sp)
	sw     t1, 0(t0)
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 56(sp)
	; defer: captured call free with 1 args
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 64(sp)
	ld     t0, 64(sp)
	lw     t1, 0(t0)
	sw     t1, 72(sp)
	; executing deferred cleanup before return
	ld     t0, 56(sp)
	addi   a0, t0, 0
	jal ra, free
	lw     t1, 72(sp)
	addi   a0, t1, 0
	ld     s0, 88(sp)
	ld     ra, 80(sp)
	addi   sp, sp, 96
	jalr   zero, 0(ra)