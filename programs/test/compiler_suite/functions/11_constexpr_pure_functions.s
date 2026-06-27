.section .text
; Function: factorial
factorial:
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
; Move parameter '$n' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
factorial__entry:
; bind parameter: n
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; if condition
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 1
	slt    t1, t0, s2
	sltiu  s2, t1, 1
	bne s2, zero, factorial__label_0
	j factorial__label_2
; Basic Block: label_0
factorial__label_0:
	addi   t2, zero, 1
	addi   a0, t2, 0
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
; Basic Block: label_2
factorial__label_2:
; Load i32 from memory into $$2
	addi   t0, sp, 0
	lw     s2, 0(t0)
; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     s3, 0(t0)
; sub operation on i32
	addi   t0, zero, 1
	sub    s3, s3, t0
	addiw  s3, s3, 0
; --- Function Call: factorial ---
; Passing 1 arguments
	addi   a0, s3, 0
	jal ra, factorial
	addiw  s3, a0, 0
; --- End Function Call: factorial ---
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
; Function: fibonacci
fibonacci:
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
; Move parameter '$n' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
fibonacci__entry:
; bind parameter: n
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; if condition
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 0
	slt    t1, t0, s2
	sltiu  s2, t1, 1
	bne s2, zero, fibonacci__label_3
	j fibonacci__label_5
; Basic Block: label_3
fibonacci__label_3:
	addi   t2, zero, 0
	addi   a0, t2, 0
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
; Basic Block: label_5
fibonacci__label_5:
; if condition
; Load i32 from memory into $$2
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 1
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, fibonacci__label_6
	j fibonacci__label_8
; Basic Block: label_6
fibonacci__label_6:
	addi   t2, zero, 1
	addi   a0, t2, 0
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
; Basic Block: label_8
fibonacci__label_8:
; Load i32 from memory into $$4
	addi   t0, sp, 0
	lw     s2, 0(t0)
; sub operation on i32
	addi   t0, zero, 1
	sub    s2, s2, t0
	addiw  s2, s2, 0
; --- Function Call: fibonacci ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, fibonacci
	addiw  s2, a0, 0
; --- End Function Call: fibonacci ---
; Load i32 from memory into $$7
	addi   t0, sp, 0
	lw     s3, 0(t0)
; sub operation on i32
	addi   t0, zero, 2
	sub    s3, s3, t0
	addiw  s3, s3, 0
; --- Function Call: fibonacci ---
; Passing 1 arguments
	addi   a0, s3, 0
	jal ra, fibonacci
	addiw  s3, a0, 0
; --- End Function Call: fibonacci ---
; add operation on i32
	add    s2, s2, s3
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
; Function: add_multiply
add_multiply:
; --- Function Prologue ---
; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
; Save return address (ra) at offset 48
	sd     ra, 48(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; Save callee-saved register s3 at offset 32
	sd     s3, 32(sp)
; Save callee-saved register s4 at offset 40
	sd     s4, 40(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 64
; Move parameter '$a' from register a0 to allocated register
	addiw  s2, a0, 0
; Move parameter '$b' from register a1 to allocated register
	addiw  s3, a1, 0
; Move parameter '$c' from register a2 to allocated register
	addiw  s4, a2, 0
; --- End Parameter Spills ---
; Basic Block: entry
add_multiply__entry:
; bind parameter: a
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; bind parameter: b
	addi   t0, sp, 8
; Store i32 to memory
	sw     s3, 0(t0)
; bind parameter: c
	addi   t0, sp, 16
; Store i32 to memory
	sw     s4, 0(t0)
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     s3, 0(t0)
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
; Load i32 from memory into $$3
	addi   t0, sp, 16
	lw     s3, 0(t0)
; mul operation on i32
	mul    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s4 from offset 40
	ld     s4, 40(sp)
; Restore callee-saved register s3 from offset 32
	ld     s3, 32(sp)
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: max_value
max_value:
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
max_value__entry:
; bind parameter: a
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; bind parameter: b
	addi   t0, sp, 8
; Store i32 to memory
	sw     s3, 0(t0)
; if condition
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     s3, 0(t0)
	slt    s2, s3, s2
	bne s2, zero, max_value__label_9
	j max_value__label_11
; Basic Block: label_9
max_value__label_9:
; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     s2, 0(t0)
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
; Basic Block: label_11
max_value__label_11:
; Load i32 from memory into $$4
	addi   t0, sp, 8
	lw     s2, 0(t0)
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
; Save return address (ra) at offset 0
	sd     ra, 0(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; --- Function Call: print ---
; Passing 1 arguments
	addi   t0, zero, 120
	addi   a0, t0, 0
	jal ra, print
; --- End Function Call: print ---
; --- Function Call: print ---
; Passing 1 arguments
	lui    t0, 0x376
	addi   t0, t0, -256
	addi   a0, t0, 0
	jal ra, print
; --- End Function Call: print ---
; --- Function Call: print ---
; Passing 1 arguments
	addi   t0, zero, 55
	addi   a0, t0, 0
	jal ra, print
; --- End Function Call: print ---
; --- Function Call: print ---
; Passing 1 arguments
	addi   t0, zero, 20
	addi   a0, t0, 0
	jal ra, print
; --- End Function Call: print ---
; --- Function Call: print ---
; Passing 1 arguments
	addi   t0, zero, 42
	addi   a0, t0, 0
	jal ra, print
; --- End Function Call: print ---
; --- Function Call: print ---
; Passing 1 arguments
	lui    t0, 0xa
	addi   t0, t0, -640
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