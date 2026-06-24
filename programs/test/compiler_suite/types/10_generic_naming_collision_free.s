.section .text
; Function: main
.globl main
main:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 32
	sd     ra, 32(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: boxed
	addi   a0, zero, 4
	call malloc
	addi   s2, a0, 0
	addi   t0, sp, 0
; Store Box<i32>* to memory
	sd     s2, 0(t0)
; local var: legacy
	addi   a0, zero, 4
	call malloc
	addi   s2, a0, 0
	addi   t0, sp, 8
; Store Box_i32* to memory
	sd     s2, 0(t0)
; local var: double
	addi   a0, zero, 4
	call malloc
	addi   s2, a0, 0
	addi   t0, sp, 16
; Store Box<Box<i32>>* to memory
	sd     s2, 0(t0)
; Load Box<i32>* from memory into $$3
	addi   t0, sp, 0
	ld     s2, 0(t0)
	addi   a0, s2, 0
	call free
; Load Box_i32* from memory into $$4
	addi   t0, sp, 8
	ld     s2, 0(t0)
	addi   a0, s2, 0
	call free
; Load Box<Box<i32>>* from memory into $$5
	addi   t0, sp, 16
	ld     s2, 0(t0)
	addi   a0, s2, 0
	call free
	addi   t0, zero, 0
	addi   a0, t0, 0
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 32
	ld     ra, 32(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function