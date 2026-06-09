.section .text
	; ========================================
	; Function: calc_offset
	; ========================================
.globl calc_offset
calc_offset:
	; --- Function Prologue ---
	; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
	; Save return address (ra) at offset 40
	sd     ra, 40(sp)
	; Save callee-saved register s8 at offset 48
	sd     s0, 48(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 64
	; Spill parameter '$p' from register a0 to stack slot 16
	sd     a0, 16(sp)
	; Spill parameter '$shift' from register a1 to stack slot 24
	fsw    fa1, 24(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
calc_offset__entry:
	; bind parameter: p
	addi   t0, sp, 0
	; Store Point* to memory
	ld     t1, 16(sp)
	sd     t1, 0(t0)
	; bind parameter: shift
	addi   t0, sp, 8
	; Store f32 to memory
	flw    ft0, 24(sp)
	fsw    ft0, 0(t0)
	; assignment
	; Load {x: f32, y: f32}* from memory into $$0
	addi   t0, sp, 0
	ld     t1, 0(t0)
	sd     t1, 16(sp)
	ld     t0, 16(sp)
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 16(sp)
	; Load {x: f32, y: f32}* from memory into $$2
	addi   t0, sp, 0
	ld     t1, 0(t0)
	sd     t1, 24(sp)
	ld     t0, 24(sp)
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 24(sp)
	; Load f32 from memory into $$4
	ld     t0, 24(sp)
	ld     t1, 0(t0)
	fsw    ft6, 24(sp)
	; Load f32 from memory into $$5
	addi   t0, sp, 8
	ld     t1, 0(t0)
	fsw    ft6, 32(sp)
	; add operation on f32
	flw    ft1, 24(sp)
	flw    ft2, 32(sp)
	fadd.s ft3, ft1, ft2
	fsw    ft3, 24(sp)
	ld     t0, 16(sp)
	; Store f32 to memory
	flw    ft4, 24(sp)
	fsw    ft4, 0(t0)
	; assignment
	; Load {x: f32, y: f32}* from memory into $$7
	addi   t0, sp, 0
	ld     t1, 0(t0)
	sd     t1, 16(sp)
	ld     t0, 16(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 16(sp)
	; Load {x: f32, y: f32}* from memory into $$9
	addi   t0, sp, 0
	ld     t1, 0(t0)
	sd     t1, 24(sp)
	ld     t0, 24(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 24(sp)
	; Load f32 from memory into $$11
	ld     t0, 24(sp)
	ld     t1, 0(t0)
	fsw    ft6, 24(sp)
	; Load f32 from memory into $$12
	addi   t0, sp, 8
	ld     t1, 0(t0)
	fsw    ft6, 32(sp)
	; add operation on f32
	flw    ft5, 24(sp)
	flw    ft6, 32(sp)
	fadd.s ft7, ft5, ft6
	fsw    ft7, 24(sp)
	ld     t0, 16(sp)
	; Store f32 to memory
	flw    ft0, 24(sp)
	fsw    ft0, 0(t0)
	; Load {x: f32, y: f32}* from memory into $$14
	addi   t0, sp, 0
	ld     t1, 0(t0)
	sd     t1, 16(sp)
	ld     t0, 16(sp)
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 16(sp)
	; Load f32 from memory into $$16
	ld     t0, 16(sp)
	ld     t1, 0(t0)
	fsw    ft6, 16(sp)
	; Load {x: f32, y: f32}* from memory into $$17
	addi   t0, sp, 0
	ld     t1, 0(t0)
	sd     t1, 24(sp)
	ld     t0, 24(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 24(sp)
	; Load f32 from memory into $$19
	ld     t0, 24(sp)
	ld     t1, 0(t0)
	fsw    ft6, 24(sp)
	; mul operation on f32
	flw    ft1, 16(sp)
	flw    ft2, 24(sp)
	fmul.s ft3, ft1, ft2
	fsw    ft3, 16(sp)
	flw    ft4, 16(sp)
	fsgnj.s fa0, ft4, ft4
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 48
	ld     s0, 48(sp)
	; Restore return address (ra) from offset 40
	ld     ra, 40(sp)
	; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function