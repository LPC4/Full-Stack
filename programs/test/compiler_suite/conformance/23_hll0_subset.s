.section .text
; Function: multiply
multiply:
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
; Move parameter '$left' from register a0 to allocated register
	addiw  s2, a0, 0
; Move parameter '$right' from register a1 to allocated register
	addiw  s3, a1, 0
; --- End Parameter Spills ---
; Basic Block: entry
multiply__entry:
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
	addi   t0, sp, 8
; Store i32 to memory
	sw     s3, 0(t0)
; local var: result
	addi   t0, sp, 16
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
; local var: count
	addi   t0, sp, 24
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	j multiply__while_cond_0
; Basic Block: while_cond_0
multiply__while_cond_0:
; Load i32 from memory into $$0
	addi   t0, sp, 24
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     s3, 0(t0)
	slt    s2, s2, s3
	bne s2, zero, multiply__while_body_1
	j multiply__while_exit_2
; Basic Block: while_body_1
multiply__while_body_1:
; Load i32 from memory into $$3
	addi   t0, sp, 16
	lw     s2, 0(t0)
; Load i32 from memory into $$4
	addi   t0, sp, 0
	lw     s3, 0(t0)
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$6
	addi   t0, sp, 24
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 24
; Store i32 to memory
	sw     s2, 0(t0)
	j multiply__while_cond_0
; Basic Block: while_exit_2
multiply__while_exit_2:
; Load i32 from memory into $$8
	addi   t0, sp, 16
	lw     s2, 0(t0)
	addi   a0, s2, 0
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
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 16
	sd     ra, 16(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: answer
; --- Function Call: multiply ---
; Passing 2 arguments
	addi   t0, zero, 6
	addi   a0, t0, 0
	addi   t1, zero, 7
	addi   a1, t1, 0
	jal ra, multiply
	addiw  s2, a0, 0
; --- End Function Call: multiply ---
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 42
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_3
	j main__if_merge_5
; Basic Block: if_then_3
main__if_then_3:
	addi   t2, zero, 1
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 16
	ld     ra, 16(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_5
main__if_merge_5:
; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 16
	ld     ra, 16(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function