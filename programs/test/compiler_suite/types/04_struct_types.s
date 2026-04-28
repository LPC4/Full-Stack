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
	addi   t0, sp, 16
	ld     t1, 0(sp)
	sd     t1, 0(t0)
	; bind parameter: shift
	addi   t0, sp, 24
	flw    ft6, 8(sp)
	fsw    ft6, 0(t0)
	; assignment
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 32(sp)
	ld     t0, 32(sp)
	addi   t1, t0, 0
	flw    ft7, 0(t1)
	fsw    ft7, 40(sp)
	addi   t0, sp, 24
	flw    ft6, 0(t0)
	fsw    ft6, 44(sp)
	flw    ft0, 40(sp)
	flw    ft1, 44(sp)
	fadd.s ft2, ft0, ft1
	fsw    ft2, 48(sp)
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 56(sp)
	ld     t0, 56(sp)
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 64(sp)
	ld     t0, 64(sp)
	flw    ft6, 48(sp)
	fsw    ft6, 0(t0)
	; assignment
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 72(sp)
	ld     t0, 72(sp)
	addi   t1, t0, 4
	flw    ft7, 0(t1)
	fsw    ft7, 80(sp)
	addi   t0, sp, 24
	flw    ft6, 0(t0)
	fsw    ft6, 84(sp)
	flw    ft0, 80(sp)
	flw    ft1, 84(sp)
	fadd.s ft2, ft0, ft1
	fsw    ft2, 88(sp)
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 96(sp)
	ld     t0, 96(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 104(sp)
	ld     t0, 104(sp)
	flw    ft6, 88(sp)
	fsw    ft6, 0(t0)
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 112(sp)
	ld     t0, 112(sp)
	addi   t1, t0, 0
	flw    ft7, 0(t1)
	fsw    ft7, 120(sp)
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 128(sp)
	ld     t0, 128(sp)
	addi   t1, t0, 4
	flw    ft7, 0(t1)
	fsw    ft7, 136(sp)
	flw    ft0, 120(sp)
	flw    ft1, 136(sp)
	fmul.s ft2, ft0, ft1
	fsw    ft2, 140(sp)
	flw    ft3, 140(sp)
	fsgnj.s fa0, ft3, ft3
	ld     s0, 152(sp)
	ld     ra, 144(sp)
	addi   sp, sp, 160
	jalr   zero, 0(ra)