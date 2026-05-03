.section .text
	; ========================================
	; Function: calc_offset
	; ========================================
.globl calc_offset
calc_offset:
	; --- Function Prologue ---
	; Allocate stack frame: 192 bytes
	addi   sp, sp, -192
	; Save return address (ra) at offset 176
	sd     ra, 176(sp)
	; Save callee-saved register s8 at offset 184
	sd     s0, 184(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 192
	; Spill parameter '$p' from register a0 to stack slot 0
	sd     a0, 0(sp)
	; Spill parameter '$shift' from register a1 to stack slot 8
	fsw    fa1, 8(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
calc_offset__entry:
	; bind parameter: p
	addi   t0, sp, 16
	; Store Point* to memory
	ld     t1, 0(sp)
	sd     t1, 0(t0)
	; bind parameter: shift
	addi   t0, sp, 24
	; Store f32 to memory
	flw    ft0, 8(sp)
	fsw    ft0, 0(t0)
	; assignment
	; Load {x: f32, y: f32}* from memory into $$0
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 32(sp)
	ld     t0, 32(sp)
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 40(sp)
	; Load f32 from memory into $$2
	ld     t0, 40(sp)
	ld     t1, 0(t0)
	fsw    ft6, 48(sp)
	; Load f32 from memory into $$3
	addi   t0, sp, 24
	ld     t1, 0(t0)
	fsw    ft6, 52(sp)
	; add operation on f32
	flw    ft1, 48(sp)
	flw    ft2, 52(sp)
	fadd.s ft3, ft1, ft2
	fsw    ft3, 56(sp)
	; Load {x: f32, y: f32}* from memory into $$5
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 64(sp)
	ld     t0, 64(sp)
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 72(sp)
	ld     t0, 72(sp)
	; Store f32 to memory
	flw    ft4, 56(sp)
	fsw    ft4, 0(t0)
	; assignment
	; Load {x: f32, y: f32}* from memory into $$7
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 80(sp)
	ld     t0, 80(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 88(sp)
	; Load f32 from memory into $$9
	ld     t0, 88(sp)
	ld     t1, 0(t0)
	fsw    ft6, 96(sp)
	; Load f32 from memory into $$10
	addi   t0, sp, 24
	ld     t1, 0(t0)
	fsw    ft6, 100(sp)
	; add operation on f32
	flw    ft5, 96(sp)
	flw    ft6, 100(sp)
	fadd.s ft7, ft5, ft6
	fsw    ft7, 104(sp)
	; Load {x: f32, y: f32}* from memory into $$12
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 112(sp)
	ld     t0, 112(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 120(sp)
	ld     t0, 120(sp)
	; Store f32 to memory
	flw    ft0, 104(sp)
	fsw    ft0, 0(t0)
	; Load {x: f32, y: f32}* from memory into $$14
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 128(sp)
	ld     t0, 128(sp)
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 136(sp)
	; Load f32 from memory into $$16
	ld     t0, 136(sp)
	ld     t1, 0(t0)
	fsw    ft6, 144(sp)
	; Load {x: f32, y: f32}* from memory into $$17
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 152(sp)
	ld     t0, 152(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 160(sp)
	; Load f32 from memory into $$19
	ld     t0, 160(sp)
	ld     t1, 0(t0)
	fsw    ft6, 168(sp)
	; mul operation on f32
	flw    ft1, 144(sp)
	flw    ft2, 168(sp)
	fmul.s ft3, ft1, ft2
	fsw    ft3, 172(sp)
	flw    ft4, 172(sp)
	fsgnj.s fa0, ft4, ft4
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 184
	ld     s0, 184(sp)
	; Restore return address (ra) from offset 176
	ld     ra, 176(sp)
	; Deallocate stack frame: 192 bytes
	addi   sp, sp, 192
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function