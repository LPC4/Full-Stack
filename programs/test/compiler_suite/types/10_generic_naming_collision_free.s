.section .text
; Function: main
.globl main
main:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 24
	sd     ra, 24(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: boxed
; local var: legacy
; local var: double
	addi   t0, zero, 0
	addi   a0, t0, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 24
	ld     ra, 24(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function