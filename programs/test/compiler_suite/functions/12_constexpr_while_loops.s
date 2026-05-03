.section .text
	; ========================================
	; Function: sum_to_n
	; ========================================
.globl sum_to_n
sum_to_n:
	; --- Function Prologue ---
	; Allocate stack frame: 96 bytes
	addi   sp, sp, -96
	; Save return address (ra) at offset 72
	sd     ra, 72(sp)
	; Save callee-saved register s8 at offset 80
	sd     s0, 80(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 96
	; Spill parameter '$n' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
sum_to_n__entry:
	; bind parameter: n
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; local var: result
	addi   t0, sp, 16
	; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	; local var: i
	addi   t0, sp, 24
	; Store i32 to memory
	addi   t1, zero, 1
	sw     t1, 0(t0)
	j sum_to_n__label_0
	; --- Basic Block: label_0 ---
sum_to_n__label_0:
	; while condition
	; Load i32 from memory into $$0
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	lw     t0, 32(sp)
	lw     t1, 36(sp)
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 40(sp)
	lb     t4, 40(sp)
	bne t4, zero, sum_to_n__label_1
	j sum_to_n__label_2
	; --- Basic Block: label_1 ---
sum_to_n__label_1:
	; assignment
	; Load i32 from memory into $$3
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 44(sp)
	; Load i32 from memory into $$4
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 48(sp)
	; add operation on i32
	lw     t0, 44(sp)
	lw     t1, 48(sp)
	add    t2, t0, t1
	sw     t2, 52(sp)
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 52(sp)
	sw     t1, 0(t0)
	; assignment
	; Load i32 from memory into $$6
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 56(sp)
	; add operation on i32
	lw     t0, 56(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 60(sp)
	addi   t0, sp, 24
	; Store i32 to memory
	lw     t1, 60(sp)
	sw     t1, 0(t0)
	j sum_to_n__label_0
	; --- Basic Block: label_2 ---
sum_to_n__label_2:
	; Load i32 from memory into $$8
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 64(sp)
	lw     t2, 64(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 80
	ld     s0, 80(sp)
	; Restore return address (ra) from offset 72
	ld     ra, 72(sp)
	; Deallocate stack frame: 96 bytes
	addi   sp, sp, 96
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function

	; ========================================
	; Function: factorial_while
	; ========================================
.globl factorial_while
factorial_while:
	; --- Function Prologue ---
	; Allocate stack frame: 96 bytes
	addi   sp, sp, -96
	; Save return address (ra) at offset 72
	sd     ra, 72(sp)
	; Save callee-saved register s8 at offset 80
	sd     s0, 80(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t3, s0, 96
	; Spill parameter '$n' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
factorial_while__entry:
	; bind parameter: n
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; local var: result
	addi   t0, sp, 16
	; Store i32 to memory
	addi   t1, zero, 1
	sw     t1, 0(t0)
	; local var: i
	addi   t0, sp, 24
	; Store i32 to memory
	addi   t1, zero, 2
	sw     t1, 0(t0)
	j factorial_while__label_3
	; --- Basic Block: label_3 ---
factorial_while__label_3:
	; while condition
	; Load i32 from memory into $$0
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	lw     t0, 32(sp)
	lw     t1, 36(sp)
	slt    t3, t1, t0
	sltiu  t2, t3, 1
	sb     t2, 40(sp)
	lb     t4, 40(sp)
	bne t4, zero, factorial_while__label_4
	j factorial_while__label_5
	; --- Basic Block: label_4 ---
factorial_while__label_4:
	; assignment
	; Load i32 from memory into $$3
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 44(sp)
	; Load i32 from memory into $$4
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 48(sp)
	; mul operation on i32
	lw     t0, 44(sp)
	lw     t1, 48(sp)
	mul    t2, t0, t1
	sw     t2, 52(sp)
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 52(sp)
	sw     t1, 0(t0)
	; assignment
	; Load i32 from memory into $$6
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 56(sp)
	; add operation on i32
	lw     t0, 56(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 60(sp)
	addi   t0, sp, 24
	; Store i32 to memory
	lw     t1, 60(sp)
	sw     t1, 0(t0)
	j factorial_while__label_3
	; --- Basic Block: label_5 ---
factorial_while__label_5:
	; Load i32 from memory into $$8
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 64(sp)
	lw     t2, 64(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 80
	ld     s0, 80(sp)
	; Restore return address (ra) from offset 72
	ld     ra, 72(sp)
	; Deallocate stack frame: 96 bytes
	addi   sp, sp, 96
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