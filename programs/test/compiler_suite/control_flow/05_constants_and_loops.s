.section .text
	; ========================================
	; Function: main
	; ========================================
.globl main
main:
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
	; --- Basic Block: entry ---
main__entry:
	; local var: c
	; add operation on i32
	addi   t0, zero, 10
	addi   t1, zero, 20
	add    t2, t0, t1
	sw     t2, 4(sp)
	addi   t0, sp, 0
	; Store i32 to memory
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	j main__label_0
	; --- Basic Block: label_0 ---
main__label_0:
	; while condition
	; Load i32 from memory into $$1
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 8(sp)
	lw     t0, 8(sp)
	addi   t1, zero, 50
	slt    t2, t0, t1
	sb     t2, 12(sp)
	lb     t3, 12(sp)
	bne t3, zero, main__label_1
	j main__label_2
	; --- Basic Block: label_1 ---
main__label_1:
	; assignment
	; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	; add operation on i32
	lw     t0, 16(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 20(sp)
	addi   t0, sp, 0
	; Store i32 to memory
	lw     t1, 20(sp)
	sw     t1, 0(t0)
	; if condition
	; Load i32 from memory into $$5
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	lw     t0, 24(sp)
	addi   t1, zero, 40
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 28(sp)
	lb     t4, 28(sp)
	bne t4, zero, main__label_3
	j main__label_5
	; --- Basic Block: label_3 ---
main__label_3:
	j main__label_2
	; --- Basic Block: label_5 ---
main__label_5:
	; if condition
	; Load i32 from memory into $$7
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t0, 32(sp)
	addi   t1, zero, 35
	sub    t3, t0, t1
	sltiu  t2, t3, 1
	sb     t2, 36(sp)
	lb     t4, 36(sp)
	bne t4, zero, main__label_6
	j main__label_8
	; --- Basic Block: label_6 ---
main__label_6:
	j main__label_0
	; --- Basic Block: label_8 ---
main__label_8:
	j main__label_0
	; --- Basic Block: label_2 ---
main__label_2:
	; Load i32 from memory into $$9
	addi   t0, sp, 0
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