.section .text
; Function: main
.globl main
main:
; --- Function Prologue ---
; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
; Save return address (ra) at offset 16
	sd     ra, 16(sp)
; Save callee-saved register s2 at offset 8
	sd     s2, 8(sp)
; Save callee-saved register s0 at offset 24
	sd     s0, 24(sp)
; Set up frame pointer
	addi   s0, sp, 0
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: c
; add operation on i32
	addi   t0, zero, 10
	addi   t1, zero, 20
	add    s2, t0, t1
	addiw  s2, s2, 0
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
	j main__label_0
; Basic Block: label_0
main__label_0:
; while condition
; Load i32 from memory into $$1
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 50
	slt    s2, s2, t0
	bne s2, zero, main__label_1
	j main__label_2
; Basic Block: label_1
main__label_1:
; assignment
; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; if condition
; Load i32 from memory into $$5
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 40
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__label_3
	j main__label_5
; Basic Block: label_3
main__label_3:
	j main__label_2
; Basic Block: label_5
main__label_5:
; if condition
; Load i32 from memory into $$7
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, zero, 35
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__label_6
	j main__label_8
; Basic Block: label_6
main__label_6:
	j main__label_0
; Basic Block: label_8
main__label_8:
	j main__label_0
; Basic Block: label_2
main__label_2:
; Load i32 from memory into $$9
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s0 from offset 24
	ld     s0, 24(sp)
; Restore callee-saved register s2 from offset 8
	ld     s2, 8(sp)
; Restore return address (ra) from offset 16
	ld     ra, 16(sp)
; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function