.section .text
; Function: calc_offset
calc_offset:
; --- Function Prologue ---
; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
; Save return address (ra) at offset 48
	sd     ra, 48(sp)
; Save callee-saved register s2 at offset 32
	sd     s2, 32(sp)
; Save callee-saved register s3 at offset 40
	sd     s3, 40(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 64
; Move parameter '$p' from register a0 to allocated register
	addi   s2, a0, 0
; Spill parameter '$shift' from register a1 to stack slot 16
	fsw    fa1, 16(sp)
; --- End Parameter Spills ---
; Basic Block: entry
calc_offset__entry:
; bind parameter: p
	addi   t0, sp, 0
; Store Point* to memory
	sd     s2, 0(t0)
; bind parameter: shift
	addi   t0, sp, 8
; Store f32 to memory
	flw    ft0, 16(sp)
	fsw    ft0, 0(t0)
; assignment
; Load {x: f32, y: f32}* from memory into $$0
	addi   t0, sp, 0
	ld     s2, 0(t0)
; Address of field 'x' at offset 0
	addi   t0, zero, 0
	addi   t1, t0, 0
	add    s2, s2, t1
; Load {x: f32, y: f32}* from memory into $$2
	addi   t0, sp, 0
	ld     s3, 0(t0)
; Access field 'x' at offset 0
; Load f32 from memory into $$3
	addi   t0, s3, 0
	flw    ft1, 0(t0)
	fsw    ft1, 16(sp)
; Load f32 from memory into $$4
	addi   t0, sp, 8
	flw    ft2, 0(t0)
	fsw    ft2, 24(sp)
; add operation on f32
	flw    ft3, 16(sp)
	flw    ft4, 24(sp)
	fadd.s ft5, ft3, ft4
	fsw    ft5, 16(sp)
; Store f32 to memory
	flw    ft6, 16(sp)
	fsw    ft6, 0(s2)
; assignment
; Load {x: f32, y: f32}* from memory into $$6
	addi   t0, sp, 0
	ld     s2, 0(t0)
; Address of field 'y' at offset 4
	addi   t0, zero, 4
	addi   t1, t0, 0
	add    s2, s2, t1
; Load {x: f32, y: f32}* from memory into $$8
	addi   t0, sp, 0
	ld     s3, 0(t0)
; Access field 'y' at offset 4
; Load f32 from memory into $$9
	addi   t0, s3, 4
	flw    ft7, 0(t0)
	fsw    ft7, 16(sp)
; Load f32 from memory into $$10
	addi   t0, sp, 8
	flw    ft0, 0(t0)
	fsw    ft0, 24(sp)
; add operation on f32
	flw    ft1, 16(sp)
	flw    ft2, 24(sp)
	fadd.s ft3, ft1, ft2
	fsw    ft3, 16(sp)
; Store f32 to memory
	flw    ft4, 16(sp)
	fsw    ft4, 0(s2)
; Load {x: f32, y: f32}* from memory into $$12
	addi   t0, sp, 0
	ld     s2, 0(t0)
; Access field 'x' at offset 0
; Load f32 from memory into $$13
	addi   t0, s2, 0
	flw    ft5, 0(t0)
	fsw    ft5, 16(sp)
; Load {x: f32, y: f32}* from memory into $$14
	addi   t0, sp, 0
	ld     s2, 0(t0)
; Access field 'y' at offset 4
; Load f32 from memory into $$15
	addi   t0, s2, 4
	flw    ft6, 0(t0)
	fsw    ft6, 24(sp)
; mul operation on f32
	flw    ft7, 16(sp)
	flw    ft0, 24(sp)
	fmul.s ft1, ft7, ft0
	fsw    ft1, 16(sp)
	flw    ft2, 16(sp)
	fsgnj.s fa0, ft2, ft2
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 40
	ld     s3, 40(sp)
; Restore callee-saved register s2 from offset 32
	ld     s2, 32(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function