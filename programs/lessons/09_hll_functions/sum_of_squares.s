.section .text
; Function: square
square:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; Save callee-saved register s3 at offset 16
	sd     s3, 16(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 32
; Move parameter '$value' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
square__entry:
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 0
	lw     s3, 0(t0)
; mul operation on i32
	mul    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 16
	ld     s3, 16(sp)
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: sum_of_squares
sum_of_squares:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 32
	sd     ra, 32(sp)
; Save callee-saved register s2 at offset 16
	sd     s2, 16(sp)
; Save callee-saved register s3 at offset 24
	sd     s3, 24(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 48
; Move parameter '$a' from register a0 to allocated register
	addiw  s2, a0, 0
; Move parameter '$b' from register a1 to allocated register
	addiw  s3, a1, 0
; --- End Parameter Spills ---
; Basic Block: entry
sum_of_squares__entry:
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
	addi   t0, sp, 8
; Store i32 to memory
	sw     s3, 0(t0)
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
; --- Function Call: square ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, square
	addiw  s2, a0, 0
; --- End Function Call: square ---
; Load i32 from memory into $$2
	addi   t0, sp, 8
	lw     s3, 0(t0)
; --- Function Call: square ---
; Passing 1 arguments
	addi   a0, s3, 0
	jal ra, square
	addiw  s3, a0, 0
; --- End Function Call: square ---
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 24
	ld     s3, 24(sp)
; Restore callee-saved register s2 from offset 16
	ld     s2, 16(sp)
; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 16 bytes
	addi   sp, sp, -16
; Save return address (ra) at offset 8
	sd     ra, 8(sp)
; Save callee-saved register s2 at offset 0
	sd     s2, 0(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; --- Function Call: sum_of_squares ---
; Passing 2 arguments
	addi   t0, zero, 3
	addi   a0, t0, 0
	addi   t1, zero, 4
	addi   a1, t1, 0
	jal ra, sum_of_squares
	addiw  s2, a0, 0
; --- End Function Call: sum_of_squares ---
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 0
	ld     s2, 0(sp)
; Restore return address (ra) from offset 8
	ld     ra, 8(sp)
; Deallocate stack frame: 16 bytes
	addi   sp, sp, 16
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function