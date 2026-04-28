.section .text
	; ========================================
	; Function: test_simple
	; ========================================
.globl test_simple
test_simple:
	; --- Function Prologue ---
	; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
	; Save return address (ra) at offset 16
	sd     ra, 16(sp)
	; Save callee-saved register s8 at offset 24
	sd     s0, 24(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Basic Block: entry ---
test_simple__entry:
	; local var: q
	addi   t0, sp, 0
	; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	; local var: r
	addi   t0, sp, 4
	; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	; assignment
	addi   t0, sp, 0
	; Store i32 to memory
	addi   t1, zero, 5
	sw     t1, 0(t0)
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t2, 8(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 24
	ld     s0, 24(sp)
	; Restore return address (ra) from offset 16
	ld     ra, 16(sp)
	; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function