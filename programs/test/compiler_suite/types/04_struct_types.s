.section .text
	; ========================================
	; Function: calc_offset
	; ========================================
.globl calc_offset
calc_offset:
	; --- Function Prologue ---
	; Allocate stack frame: 160 bytes
	addi   sp, sp, -160
	; Save return address (ra) at offset 144
	sd     ra, 144(sp)
	; Save callee-saved register s8 at offset 152
	sd     s0, 152(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 160
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
	; Load Point* from memory into $$0
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 32(sp)
	; Access field 'x' at offset 0
	; Load f32 from memory into $$1
	ld     t0, 32(sp)
	addi   t1, t0, 0
	flw    ft7, 0(t1)
	fsw    ft7, 40(sp)
	; Load f32 from memory into $$2
	addi   t0, sp, 24
	flw    ft6, 0(t0)
	fsw    ft6, 44(sp)
	; add operation on f32
	flw    ft1, 40(sp)
	flw    ft2, 44(sp)
	fadd.s ft3, ft1, ft2
	fsw    ft3, 48(sp)
	; Load Point* from memory into $$4
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 56(sp)
	; Address of field 'x' at offset 0
	ld     t0, 56(sp)
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 64(sp)
	ld     t0, 64(sp)
	; Store f32 to memory
	flw    ft4, 48(sp)
	fsw    ft4, 0(t0)
	; assignment
	; Load Point* from memory into $$6
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 72(sp)
	; Access field 'y' at offset 4
	; Load f32 from memory into $$7
	ld     t0, 72(sp)
	addi   t1, t0, 4
	flw    ft7, 0(t1)
	fsw    ft7, 80(sp)
	; Load f32 from memory into $$8
	addi   t0, sp, 24
	flw    ft6, 0(t0)
	fsw    ft6, 84(sp)
	; add operation on f32
	flw    ft5, 80(sp)
	flw    ft6, 84(sp)
	fadd.s ft7, ft5, ft6
	fsw    ft7, 88(sp)
	; Load Point* from memory into $$10
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 96(sp)
	; Address of field 'y' at offset 4
	ld     t0, 96(sp)
	addi   t1, zero, 4
	addi   t2, t1, 0
	add    t3, t0, t2
	sd     t3, 104(sp)
	ld     t0, 104(sp)
	; Store f32 to memory
	flw    ft0, 88(sp)
	fsw    ft0, 0(t0)
	; Load Point* from memory into $$12
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 112(sp)
	; Access field 'x' at offset 0
	; Load f32 from memory into $$13
	ld     t0, 112(sp)
	addi   t1, t0, 0
	flw    ft7, 0(t1)
	fsw    ft7, 120(sp)
	; Load Point* from memory into $$14
	addi   t0, sp, 16
	ld     t1, 0(t0)
	sd     t1, 128(sp)
	; Access field 'y' at offset 4
	; Load f32 from memory into $$15
	ld     t0, 128(sp)
	addi   t1, t0, 4
	flw    ft7, 0(t1)
	fsw    ft7, 136(sp)
	; mul operation on f32
	flw    ft1, 120(sp)
	flw    ft2, 136(sp)
	fmul.s ft3, ft1, ft2
	fsw    ft3, 140(sp)
	flw    ft4, 140(sp)
	fsgnj.s fa0, ft4, ft4
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 152
	ld     s0, 152(sp)
	; Restore return address (ra) from offset 144
	ld     ra, 144(sp)
	; Deallocate stack frame: 160 bytes
	addi   sp, sp, 160
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function