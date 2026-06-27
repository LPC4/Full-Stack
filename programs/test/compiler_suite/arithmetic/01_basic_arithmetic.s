.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
; Save return address (ra) at offset 64
	sd     ra, 64(sp)
; Save callee-saved register s2 at offset 40
	sd     s2, 40(sp)
; Save callee-saved register s3 at offset 48
	sd     s3, 48(sp)
; Save callee-saved register s4 at offset 56
	sd     s4, 56(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: a
	addi   t0, sp, 0
; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
; local var: b
	addi   t0, sp, 8
; Store i32 to memory
	addi   t1, zero, 20
	sw     t1, 0(t0)
; local var: c
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     s3, 0(t0)
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
; mul operation on i32
	addi   t0, zero, 2
	mul    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
; local var: d
; Load i32 from memory into $$4
	addi   t0, sp, 16
	lw     s2, 0(t0)
; sdiv operation on i32
	addi   t0, zero, 5
	div    s2, s2, t0
	addiw  s2, s2, 0
; Load i32 from memory into $$6
	addi   t0, sp, 0
	lw     s3, 0(t0)
; Load i32 from memory into $$7
	addi   t0, sp, 8
	lw     s4, 0(t0)
; mod operation on i32
	rem    s3, s3, s4
	addiw  s3, s3, 0
; sub operation on i32
	sub    s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 24
; Store i32 to memory
	sw     s2, 0(t0)
; local var: e
; Load i32 from memory into $$10
	addi   t0, sp, 24
	lw     s2, 0(t0)
	sub    s2, zero, s2
	addiw  s2, s2, 0
	addi   t0, sp, 32
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$12
	addi   t0, sp, 32
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s4 from offset 56
	ld     s4, 56(sp)
; Restore callee-saved register s3 from offset 48
	ld     s3, 48(sp)
; Restore callee-saved register s2 from offset 40
	ld     s2, 40(sp)
; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function