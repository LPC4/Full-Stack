.section .text
	; ========================================
	; Function: factorial
	; ========================================
.globl factorial
factorial:
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
	addi   t0, s0, 64
	; Spill parameter '$n' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
factorial__entry:
	; bind parameter: n
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; if condition
	; Load i32 from memory into $$0
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	lw     t0, 16(sp)
	addi   t1, zero, 1
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 20(sp)
	lb     t4, 20(sp)
	bne t4, zero, factorial__label_0
	j factorial__label_2
	; --- Basic Block: label_0 ---
factorial__label_0:
	addi   t5, zero, 1
	addi   a0, t5, 0
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
	; --- Basic Block: label_2 ---
factorial__label_2:
	; Load i32 from memory into $$2
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	; Load i32 from memory into $$3
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	; sub operation on i32
	lw     t0, 28(sp)
	addi   t1, zero, 1
	sub    t2, t0, t1
	sw     t2, 32(sp)
	; --- Function Call: factorial ---
	; Passing 1 arguments
	lw     t0, 32(sp)
	addi   a0, t0, 0
	jal ra, factorial
	sw     a0, 36(sp)
	; --- End Function Call: factorial ---
	; mul operation on i32
	lw     t0, 24(sp)
	lw     t1, 36(sp)
	mul    t2, t0, t1
	sw     t2, 40(sp)
	lw     t3, 40(sp)
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
	; Function: fibonacci
	; ========================================
.globl fibonacci
fibonacci:
	; --- Function Prologue ---
	; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
	; Save return address (ra) at offset 64
	sd     ra, 64(sp)
	; Save callee-saved register s8 at offset 72
	sd     s0, 72(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t4, s0, 80
	; Spill parameter '$n' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
fibonacci__entry:
	; bind parameter: n
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; if condition
	; Load i32 from memory into $$0
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	lw     t0, 16(sp)
	addi   t1, zero, 0
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 20(sp)
	lb     t4, 20(sp)
	bne t4, zero, fibonacci__label_3
	j fibonacci__label_5
	; --- Basic Block: label_3 ---
fibonacci__label_3:
	addi   t5, zero, 0
	addi   a0, t5, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 72
	ld     s0, 72(sp)
	; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; --- Basic Block: label_5 ---
fibonacci__label_5:
	; if condition
	; Load i32 from memory into $$2
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	lw     t0, 24(sp)
	addi   t1, zero, 1
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 28(sp)
	lb     t4, 28(sp)
	bne t4, zero, fibonacci__label_6
	j fibonacci__label_8
	; --- Basic Block: label_6 ---
fibonacci__label_6:
	addi   t5, zero, 1
	addi   a0, t5, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 72
	ld     s0, 72(sp)
	; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; --- Basic Block: label_8 ---
fibonacci__label_8:
	; Load i32 from memory into $$4
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	; sub operation on i32
	lw     t0, 32(sp)
	addi   t1, zero, 1
	sub    t2, t0, t1
	sw     t2, 36(sp)
	; --- Function Call: fibonacci ---
	; Passing 1 arguments
	lw     t0, 36(sp)
	addi   a0, t0, 0
	jal ra, fibonacci
	sw     a0, 40(sp)
	; --- End Function Call: fibonacci ---
	; Load i32 from memory into $$7
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 44(sp)
	; sub operation on i32
	lw     t0, 44(sp)
	addi   t1, zero, 2
	sub    t2, t0, t1
	sw     t2, 48(sp)
	; --- Function Call: fibonacci ---
	; Passing 1 arguments
	lw     t0, 48(sp)
	addi   a0, t0, 0
	jal ra, fibonacci
	sw     a0, 52(sp)
	; --- End Function Call: fibonacci ---
	; add operation on i32
	lw     t0, 40(sp)
	lw     t1, 52(sp)
	add    t2, t0, t1
	sw     t2, 56(sp)
	lw     t3, 56(sp)
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 72
	ld     s0, 72(sp)
	; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
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
	; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
	; Save return address (ra) at offset 64
	sd     ra, 64(sp)
	; Save callee-saved register s8 at offset 72
	sd     s0, 72(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t4, s0, 80
	; Spill parameter '$a' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; Spill parameter '$b' from register a1 to stack slot 4
	sw     a1, 4(sp)
	; Spill parameter '$c' from register a2 to stack slot 8
	sw     a2, 8(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
add_multiply__entry:
	; bind parameter: a
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; bind parameter: b
	addi   t0, sp, 24
	; Store i32 to memory
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	; bind parameter: c
	addi   t0, sp, 32
	; Store i32 to memory
	lw     t1, 8(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$0
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 44(sp)
	; add operation on i32
	lw     t0, 40(sp)
	lw     t1, 44(sp)
	add    t2, t0, t1
	sw     t2, 48(sp)
	; Load i32 from memory into $$3
	addi   t0, sp, 32
	lw     t1, 0(t0)
	sw     t1, 52(sp)
	; mul operation on i32
	lw     t0, 48(sp)
	lw     t1, 52(sp)
	mul    t2, t0, t1
	sw     t2, 56(sp)
	lw     t3, 56(sp)
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 72
	ld     s0, 72(sp)
	; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
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
	; Spill parameter '$a' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; Spill parameter '$b' from register a1 to stack slot 4
	sw     a1, 4(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
max_value__entry:
	; bind parameter: a
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; bind parameter: b
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	; if condition
	; Load i32 from memory into $$0
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	lw     t0, 24(sp)
	lw     t1, 28(sp)
	slt    t2, t1, t0
	sb     t2, 32(sp)
	lb     t3, 32(sp)
	bne t3, zero, max_value__label_9
	j max_value__label_11
	; --- Basic Block: label_9 ---
max_value__label_9:
	; Load i32 from memory into $$3
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	lw     t2, 36(sp)
	addi   a0, t2, 0
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
	; --- Basic Block: label_11 ---
max_value__label_11:
	; Load i32 from memory into $$4
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	lw     t2, 40(sp)
	addi   a0, t2, 0
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