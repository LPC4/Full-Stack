.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 16 bytes
	addi   sp, sp, -16
; Save return address (ra) at offset 8
	sd     ra, 8(sp)
; Save callee-saved register s2 at offset 0
	sd     s2, 0(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; add operation on i32
	addi   t0, zero, 1
	addi   t1, zero, 1
	add    s2, t0, t1
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 0
	ld     s2, 0(sp)
; Restore return address (ra) from offset 8
	ld     ra, 8(sp)
; Deallocate stack frame: 16 bytes
	addi   sp, sp, 16
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function