.section .text
.globl calc_offset
calc_offset:
	addi   sp, sp, -160
	sd     ra, 144(sp)
	sd     s0, 152(sp)
	addi   s0, sp, 0
	addi   t0, s0, 160
	sd     a0, 0(sp)
	fsw    fa1, 8(sp)
calc_offset__entry:
	; bind parameter: p
	addi   t1, sp, 16
	ld     t2, 0(sp)
	sd     t2, 0(t1)
	; bind parameter: shift
	addi   t3, sp, 24
	flw    ft9, 8(sp)
	fsw    ft9, 0(t3)
	; assignment
	addi   t5, sp, 16
	ld     t6, 0(t5)
	sd     t6, 32(sp)
	ld     t0, 32(sp)
	addi   t1, t0, 0
	flw    ft7, 0(t1)
	fsw    ft7, 40(sp)
	addi   t3, sp, 24
	flw    ft9, 0(t3)
	fsw    ft9, 44(sp)
	flw    ft10, 40(sp)
	flw    ft11, 44(sp)
	add    t0, t5, t6
	sd     t0, 48(sp)
	addi   t1, sp, 16
	ld     t2, 0(t1)
	sd     t2, 56(sp)
	ld     t3, 56(sp)
	addi   t4, zero, 0
	addi   t5, t4, 0
	add    t6, t3, t5
	sd     t6, 64(sp)
	ld     t0, 64(sp)
	flw    ft6, 48(sp)
	fsw    ft6, 0(t0)
	; assignment
	addi   t2, sp, 16
	ld     t3, 0(t2)
	sd     t3, 72(sp)
	ld     t4, 72(sp)
	addi   t5, t4, 4
	flw    ft11, 0(t5)
	fsw    ft11, 80(sp)
	addi   t0, sp, 24
	flw    ft6, 0(t0)
	fsw    ft6, 84(sp)
	flw    ft7, 80(sp)
	flw    ft8, 84(sp)
	add    t4, t2, t3
	sd     t4, 88(sp)
	addi   t5, sp, 16
	ld     t6, 0(t5)
	sd     t6, 96(sp)
	ld     t0, 96(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 104(sp)
	ld     t4, 104(sp)
	flw    ft10, 88(sp)
	fsw    ft10, 0(t4)
	addi   t6, sp, 16
	ld     t0, 0(t6)
	sd     t0, 112(sp)
	ld     t1, 112(sp)
	addi   t2, t1, 0
	flw    ft8, 0(t2)
	fsw    ft8, 120(sp)
	addi   t4, sp, 16
	ld     t5, 0(t4)
	sd     t5, 128(sp)
	ld     t6, 128(sp)
	addi   t0, t6, 4
	flw    ft6, 0(t0)
	fsw    ft6, 136(sp)
	flw    ft7, 120(sp)
	flw    ft8, 136(sp)
	mul    t4, t2, t3
	sd     t4, 140(sp)
	flw    ft10, 140(sp)
	addi   a0, t5, 0
	ld     s0, 152(sp)
	ld     ra, 144(sp)
	addi   sp, sp, 160
	jalr   zero, 0(ra)