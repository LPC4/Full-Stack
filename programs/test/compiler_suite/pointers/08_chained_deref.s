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
	; local var: pp
	addi   t5, sp, 24
	addi   t6, sp, 8
	sd     t6, 0(t5)
	; assignment
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t2, 32(sp)
	addi   t3, zero, 1
	add    t4, t2, t3
	sd     t4, 36(sp)
	addi   t5, sp, 24
	ld     t6, 0(t5)
	sd     t6, 40(sp)
	ld     t0, 40(sp)
	ld     t1, 0(t0)
	sd     t1, 48(sp)
	ld     t2, 48(sp)
	lw     t3, 36(sp)
	sw     t3, 0(t2)
	addi   t4, sp, 8
	ld     t5, 0(t4)
	sd     t5, 56(sp)
	; defer: captured call free with 1 args
	addi   t6, sp, 8
	ld     t0, 0(t6)
	sd     t0, 64(sp)
	ld     t1, 64(sp)
	lw     t2, 0(t1)
	sw     t2, 72(sp)
	; executing deferred cleanup before return
	ld     t3, 56(sp)
	addi   a0, t3, 0
	jal ra, free
	lw     t4, 72(sp)
	addi   a0, t4, 0
	ld     s0, 88(sp)
	ld     ra, 80(sp)
	addi   sp, sp, 96
	jalr   zero, 0(ra)