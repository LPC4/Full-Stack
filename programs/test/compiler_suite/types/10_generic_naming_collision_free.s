.section .text
; Function: main
.globl main
main:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; Save callee-saved register s0 at offset 32
	sd     s0, 32(sp)
; Set up frame pointer
	addi   s0, sp, 0
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: boxed
; local var: legacy
; local var: double
	addi   t0, zero, 0
	addi   a0, t0, 0
; --- Function Epilogue ---
; Restore callee-saved register s0 from offset 32
	ld     s0, 32(sp)
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function