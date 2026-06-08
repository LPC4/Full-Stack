.section .text
	; ========================================
	; Function: factorial
	; ========================================
.globl factorial
factorial:
	; --- Function Prologue ---
	; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
	; Save return address (ra) at offset 24
	sd     ra, 24(sp)
	; Save callee-saved register s8 at offset 32
	sd     s0, 32(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 48
	; Spill parameter '$n' from register a0 to stack slot 8
	sw     a0, 8(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
factorial__entry:
	; bind parameter: n
	addi   t0, sp, 0
	; Store i32 to memory
	lw     t1, 8(sp)
	sw     t1, 0(t0)
	; if condition
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 1
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 8(sp)
	lb     t4, 8(sp)
	bne t4, zero, factorial__label_0
	j factorial__label_2
	; --- Basic Block: label_0 ---
factorial__label_0:
	addi   t5, zero, 1
	addi   a0, t5, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 32
	ld     s0, 32(sp)
	; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
	; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; --- Basic Block: label_2 ---
factorial__label_2:
	; Load i32 from memory into $$2
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	; sub operation on i32
	lw     t0, 16(sp)
	addi   t1, zero, 1
	sub    t2, t0, t1
	sw     t2, 16(sp)
	; --- Function Call: factorial ---
	; Passing 1 arguments
	lw     t0, 16(sp)
	addi   a0, t0, 0
	jal ra, factorial
	sw     a0, 16(sp)
	; --- End Function Call: factorial ---
	; mul operation on i32
	lw     t0, 8(sp)
	lw     t1, 16(sp)
	mul    t2, t0, t1
	sw     t2, 8(sp)
	lw     t3, 8(sp)
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 32
	ld     s0, 32(sp)
	; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
	; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function

	; ========================================
	; Function: fibonacci
	; ========================================
.globl fibonacci
fibonacci:
	; --- Function Prologue ---
	; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
	; Save return address (ra) at offset 24
	sd     ra, 24(sp)
	; Save callee-saved register s8 at offset 32
	sd     s0, 32(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t4, s0, 48
	; Spill parameter '$n' from register a0 to stack slot 8
	sw     a0, 8(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
fibonacci__entry:
	; bind parameter: n
	addi   t0, sp, 0
	; Store i32 to memory
	lw     t1, 8(sp)
	sw     t1, 0(t0)
	; if condition
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 0
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 8(sp)
	lb     t4, 8(sp)
	bne t4, zero, fibonacci__label_3
	j fibonacci__label_5
	; --- Basic Block: label_3 ---
fibonacci__label_3:
	addi   t5, zero, 0
	addi   a0, t5, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 32
	ld     s0, 32(sp)
	; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
	; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; --- Basic Block: label_5 ---
fibonacci__label_5:
	; if condition
	; Load i32 from memory into $$2
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 1
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 8(sp)
	lb     t4, 8(sp)
	bne t4, zero, fibonacci__label_6
	j fibonacci__label_8
	; --- Basic Block: label_6 ---
fibonacci__label_6:
	addi   t5, zero, 1
	addi   a0, t5, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 32
	ld     s0, 32(sp)
	; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
	; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; --- Basic Block: label_8 ---
fibonacci__label_8:
	; Load i32 from memory into $$4
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	; sub operation on i32
	lw     t0, 8(sp)
	addi   t1, zero, 1
	sub    t2, t0, t1
	sw     t2, 8(sp)
	; --- Function Call: fibonacci ---
	; Passing 1 arguments
	lw     t0, 8(sp)
	addi   a0, t0, 0
	jal ra, fibonacci
	sw     a0, 8(sp)
	; --- End Function Call: fibonacci ---
	; Load i32 from memory into $$7
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	; sub operation on i32
	lw     t0, 16(sp)
	addi   t1, zero, 2
	sub    t2, t0, t1
	sw     t2, 16(sp)
	; --- Function Call: fibonacci ---
	; Passing 1 arguments
	lw     t0, 16(sp)
	addi   a0, t0, 0
	jal ra, fibonacci
	sw     a0, 16(sp)
	; --- End Function Call: fibonacci ---
	; add operation on i32
	lw     t0, 8(sp)
	lw     t1, 16(sp)
	add    t2, t0, t1
	sw     t2, 8(sp)
	lw     t3, 8(sp)
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 32
	ld     s0, 32(sp)
	; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
	; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function

	; ========================================
	; Function: add_multiply
	; ========================================
.globl add_multiply
add_multiply:
	; --- Function Prologue ---
	; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
	; Save return address (ra) at offset 48
	sd     ra, 48(sp)
	; Save callee-saved register s8 at offset 56
	sd     s0, 56(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t4, s0, 64
	; Spill parameter '$a' from register a0 to stack slot 24
	sw     a0, 24(sp)
	; Spill parameter '$b' from register a1 to stack slot 32
	sw     a1, 32(sp)
	; Spill parameter '$c' from register a2 to stack slot 40
	sw     a2, 40(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
add_multiply__entry:
	; bind parameter: a
	addi   t0, sp, 0
	; Store i32 to memory
	lw     t1, 24(sp)
	sw     t1, 0(t0)
	; bind parameter: b
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 32(sp)
	sw     t1, 0(t0)
	; bind parameter: c
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 40(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	; add operation on i32
	lw     t0, 24(sp)
	lw     t1, 32(sp)
	add    t2, t0, t1
	sw     t2, 24(sp)
	; Load i32 from memory into $$3
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	; mul operation on i32
	lw     t0, 24(sp)
	lw     t1, 32(sp)
	mul    t2, t0, t1
	sw     t2, 24(sp)
	lw     t3, 24(sp)
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 56
	ld     s0, 56(sp)
	; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
	; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function

	; ========================================
	; Function: max_value
	; ========================================
.globl max_value
max_value:
	; --- Function Prologue ---
	; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
	; Save return address (ra) at offset 32
	sd     ra, 32(sp)
	; Save callee-saved register s8 at offset 40
	sd     s0, 40(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t4, s0, 48
	; Spill parameter '$a' from register a0 to stack slot 16
	sw     a0, 16(sp)
	; Spill parameter '$b' from register a1 to stack slot 24
	sw     a1, 24(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
max_value__entry:
	; bind parameter: a
	addi   t0, sp, 0
	; Store i32 to memory
	lw     t1, 16(sp)
	sw     t1, 0(t0)
	; bind parameter: b
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 24(sp)
	sw     t1, 0(t0)
	; if condition
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	lw     t0, 16(sp)
	lw     t1, 24(sp)
	slt    t2, t1, t0
	sb     t2, 16(sp)
	lb     t3, 16(sp)
	bne t3, zero, max_value__label_9
	j max_value__label_11
	; --- Basic Block: label_9 ---
max_value__label_9:
	; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	lw     t2, 16(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 40
	ld     s0, 40(sp)
	; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
	; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; --- Basic Block: label_11 ---
max_value__label_11:
	; Load i32 from memory into $$4
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	lw     t2, 16(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 40
	ld     s0, 40(sp)
	; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
	; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function

	; ========================================
	; Function: main
	; ========================================
.globl main
main:
	; --- Function Prologue ---
	; Allocate stack frame: 16 bytes
	addi   sp, sp, -16
	; Save return address (ra) at offset 0
	sd     ra, 0(sp)
	; Save callee-saved register s8 at offset 8
	sd     s0, 8(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Basic Block: entry ---
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
	; Restore callee-saved register s8 from offset 8
	ld     s0, 8(sp)
	; Restore return address (ra) from offset 0
	ld     ra, 0(sp)
	; Deallocate stack frame: 16 bytes
	addi   sp, sp, 16
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function