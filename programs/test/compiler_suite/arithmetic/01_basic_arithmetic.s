.section .text
	; ========================================
	; Function: main
	; ========================================
.globl main
main:
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
	; --- Basic Block: entry ---
main__entry:
	; local var: a
	addi   t0, sp, 0
	; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: b
	addi   t0, sp, 4
	; Store i32 to memory
	addi   t1, zero, 20
	sw     t1, 0(t0)
	; local var: c
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 12(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	; add operation on i32
	lw     t0, 12(sp)
	lw     t1, 16(sp)
	add    t2, t0, t1
	sw     t2, 20(sp)
	; mul operation on i32
	lw     t0, 20(sp)
	addi   t1, zero, 2
	mul    t2, t0, t1
	sw     t2, 24(sp)
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 24(sp)
	sw     t1, 0(t0)
	; local var: d
	; Load i32 from memory into $$4
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	; sdiv operation on i32
	lw     t0, 32(sp)
	addi   t1, zero, 5
	div    t2, t0, t1
	sw     t2, 36(sp)
	; Load i32 from memory into $$6
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	; Load i32 from memory into $$7
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 44(sp)
	; mod operation on i32
	lw     t0, 40(sp)
	lw     t1, 44(sp)
	rem    t2, t0, t1
	sw     t2, 48(sp)
	; sub operation on i32
	lw     t0, 36(sp)
	lw     t1, 48(sp)
	sub    t2, t0, t1
	sw     t2, 52(sp)
	addi   t0, sp, 28
	; Store i32 to memory
	lw     t1, 52(sp)
	sw     t1, 0(t0)
	; local var: e
	; Load i32 from memory into $$10
	addi   t0, sp, 28
	lw     t1, 0(t0)
	sw     t1, 60(sp)
	lw     t0, 60(sp)
	sub    t1, zero, t0
	sw     t1, 64(sp)
	addi   t0, sp, 56
	; Store i32 to memory
	lw     t1, 64(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$12
	addi   t0, sp, 56
	lw     t1, 0(t0)
	sw     t1, 68(sp)
	lw     t2, 68(sp)
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