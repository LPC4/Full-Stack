.section .text
; Function: test_simple
.globl test_simple
test_simple:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s2 at offset 16
	sd     s2, 16(sp)
; --- End Prologue ---
; Basic Block: entry
test_simple__entry:
; local var: q
	addi   t0, sp, 0
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
; local var: r
	addi   t0, sp, 8
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
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 16
	ld     s2, 16(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function