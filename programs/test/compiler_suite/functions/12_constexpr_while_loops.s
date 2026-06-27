.section .text
; Function: sum_to_n
sum_to_n:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 40
	sd     ra, 40(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; Save callee-saved register s3 at offset 32
	sd     s3, 32(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 48
; Move parameter '$n' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
sum_to_n__entry:
; bind parameter: n
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; local var: result
	addi   t0, sp, 8
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
; local var: i
	addi   t0, sp, 16
; Store i32 to memory
	addi   t1, zero, 1
	sw     t1, 0(t0)
	j sum_to_n__label_0
; Basic Block: label_0
sum_to_n__label_0:
; while condition
; Load i32 from memory into $$0
	addi   t0, sp, 16
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 0
	lw     s3, 0(t0)
	slt    t0, s3, s2
	sltiu  s2, t0, 1
	bne s2, zero, sum_to_n__label_1
	j sum_to_n__label_2
; Basic Block: label_1
sum_to_n__label_1:
; assignment
; Load i32 from memory into $$3
	addi   t0, sp, 8
	lw     s2, 0(t0)
; Load i32 from memory into $$4
	addi   t0, sp, 16
	lw     s3, 0(t0)
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 8
; Store i32 to memory
	sw     s2, 0(t0)
; assignment
; Load i32 from memory into $$6
	addi   t0, sp, 16
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
	j sum_to_n__label_0
; Basic Block: label_2
sum_to_n__label_2:
; Load i32 from memory into $$8
	addi   t0, sp, 8
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 32
	ld     s3, 32(sp)
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 40
	ld     ra, 40(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: factorial_while
factorial_while:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 40
	sd     ra, 40(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; Save callee-saved register s3 at offset 32
	sd     s3, 32(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t1, sp, 48
; Move parameter '$n' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
factorial_while__entry:
; bind parameter: n
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; local var: result
	addi   t0, sp, 8
; Store i32 to memory
	addi   t1, zero, 1
	sw     t1, 0(t0)
; local var: i
	addi   t0, sp, 16
; Store i32 to memory
	addi   t1, zero, 2
	sw     t1, 0(t0)
	j factorial_while__label_3
; Basic Block: label_3
factorial_while__label_3:
; while condition
; Load i32 from memory into $$0
	addi   t0, sp, 16
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 0
	lw     s3, 0(t0)
	slt    t0, s3, s2
	sltiu  s2, t0, 1
	bne s2, zero, factorial_while__label_4
	j factorial_while__label_5
; Basic Block: label_4
factorial_while__label_4:
; assignment
; Load i32 from memory into $$3
	addi   t0, sp, 8
	lw     s2, 0(t0)
; Load i32 from memory into $$4
	addi   t0, sp, 16
	lw     s3, 0(t0)
; mul operation on i32
	mul    s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 8
; Store i32 to memory
	sw     s2, 0(t0)
; assignment
; Load i32 from memory into $$6
	addi   t0, sp, 16
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
	j factorial_while__label_3
; Basic Block: label_5
factorial_while__label_5:
; Load i32 from memory into $$8
	addi   t0, sp, 8
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 32
	ld     s3, 32(sp)
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 40
	ld     ra, 40(sp)
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
; Save return address (ra) at offset 0
	sd     ra, 0(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; --- Function Call: print ---
; Passing 1 arguments
	addi   t0, zero, 55
	addi   a0, t0, 0
	jal ra, print
; --- End Function Call: print ---
; --- Function Call: print ---
; Passing 1 arguments
	lui    t0, 0x1
	addi   t0, t0, 944
	addi   a0, t0, 0
	jal ra, print
; --- End Function Call: print ---
; --- Function Epilogue ---
; Restore return address (ra) from offset 0
	ld     ra, 0(sp)
; Deallocate stack frame: 16 bytes
	addi   sp, sp, 16
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function