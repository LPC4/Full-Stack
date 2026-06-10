.section .text
; Function: calc_offset
.globl calc_offset
calc_offset:
; --- Function Prologue ---
; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
; Save return address (ra) at offset 48
	sd     ra, 48(sp)
; Save callee-saved register s2 at offset 16
	sd     s2, 16(sp)
; Save callee-saved register s3 at offset 24
	sd     s3, 24(sp)
; Save callee-saved register s0 at offset 56
	sd     s0, 56(sp)
; Set up frame pointer
	addi   s0, sp, 0
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, s0, 64
; Move parameter '$p' from register a0 to allocated register
	addi   s2, a0, 0
; Spill parameter '$shift' from register a1 to stack slot 32
	fsw    fa1, 32(sp)
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
	flw    ft0, 32(sp)
	fsw    ft0, 0(t0)
; assignment
; Load {x: f32, y: f32}* from memory into $$0
	addi   t0, sp, 0
	ld     s2, 0(t0)
	addi   t0, zero, 0
	addi   t1, t0, 0
	add    s2, s2, t1
; Load {x: f32, y: f32}* from memory into $$2
	addi   t0, sp, 0
	ld     s3, 0(t0)
	addi   t0, zero, 0
	addi   t1, t0, 0
	add    s3, s3, t1
; Load f32 from memory into $$4
	flw    ft1, 0(s3)
	fsw    ft1, 32(sp)
; Load f32 from memory into $$5
	addi   t0, sp, 8
	flw    ft2, 0(t0)
	fsw    ft2, 40(sp)
; add operation on f32
	flw    ft3, 32(sp)
	flw    ft4, 40(sp)
	fadd.s ft5, ft3, ft4
	fsw    ft5, 32(sp)
; Store f32 to memory
	flw    ft6, 32(sp)
	fsw    ft6, 0(s2)
; assignment
; Load {x: f32, y: f32}* from memory into $$7
	addi   t0, sp, 0
	ld     s2, 0(t0)
	addi   t0, zero, 4
	addi   t1, t0, 0
	add    s2, s2, t1
; Load {x: f32, y: f32}* from memory into $$9
	addi   t0, sp, 0
	ld     s3, 0(t0)
	addi   t0, zero, 4
	addi   t1, t0, 0
	add    s3, s3, t1
; Load f32 from memory into $$11
	flw    ft7, 0(s3)
	fsw    ft7, 32(sp)
; Load f32 from memory into $$12
	addi   t0, sp, 8
	flw    ft0, 0(t0)
	fsw    ft0, 40(sp)
; add operation on f32
	flw    ft1, 32(sp)
	flw    ft2, 40(sp)
	fadd.s ft3, ft1, ft2
	fsw    ft3, 32(sp)
; Store f32 to memory
	flw    ft4, 32(sp)
	fsw    ft4, 0(s2)
; Load {x: f32, y: f32}* from memory into $$14
	addi   t0, sp, 0
	ld     s2, 0(t0)
	addi   t0, zero, 0
	addi   t1, t0, 0
	add    s2, s2, t1
; Load f32 from memory into $$16
	flw    ft5, 0(s2)
	fsw    ft5, 32(sp)
; Load {x: f32, y: f32}* from memory into $$17
	addi   t0, sp, 0
	ld     s2, 0(t0)
	addi   t0, zero, 4
	addi   t1, t0, 0
	add    s2, s2, t1
; Load f32 from memory into $$19
	flw    ft6, 0(s2)
	fsw    ft6, 40(sp)
; mul operation on f32
	flw    ft7, 32(sp)
	flw    ft0, 40(sp)
	fmul.s ft1, ft7, ft0
	fsw    ft1, 32(sp)
	flw    ft2, 32(sp)
	fsgnj.s fa0, ft2, ft2
; --- Function Epilogue ---
; Restore callee-saved register s0 from offset 56
	ld     s0, 56(sp)
; Restore callee-saved register s3 from offset 24
	ld     s3, 24(sp)
; Restore callee-saved register s2 from offset 16
	ld     s2, 16(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function