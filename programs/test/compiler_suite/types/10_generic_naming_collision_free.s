.section .text
	; ========================================
	; Function: main
	; ========================================
.globl main
main:
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
main__entry:
	; local var: boxed
	; local var: legacy
	; local var: double
	addi   t0, zero, 0
	addi   a0, t0, 0
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