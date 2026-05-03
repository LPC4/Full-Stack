.section .text
	; ========================================
	; Function: main
	; ========================================
.globl main
main:
	; --- Function Prologue ---
	; Allocate stack frame: 112 bytes
	addi   sp, sp, -112
	; Save return address (ra) at offset 96
	sd     ra, 96(sp)
	; Save callee-saved register s8 at offset 104
	sd     s0, 104(sp)
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
	addi   t0, sp, 8
	; Store i32 to memory
	addi   t1, zero, 20
	sw     t1, 0(t0)
	; local var: c
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	; add operation on i32
	lw     t0, 24(sp)
	lw     t1, 28(sp)
	add    t2, t0, t1
	sw     t2, 32(sp)
	; mul operation on i32
	lw     t0, 32(sp)
	addi   t1, zero, 2
	mul    t2, t0, t1
	sw     t2, 36(sp)
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 36(sp)
	sw     t1, 0(t0)
	; local var: d
	; Load i32 from memory into $$4
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 48(sp)
	; sdiv operation on i32
	lw     t0, 48(sp)
	addi   t1, zero, 5
	div    t2, t0, t1
	sw     t2, 52(sp)
	; Load i32 from memory into $$6
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 56(sp)
	; Load i32 from memory into $$7
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 60(sp)
	; mod operation on i32
	lw     t0, 56(sp)
	lw     t1, 60(sp)
	rem    t2, t0, t1
	sw     t2, 64(sp)
	; sub operation on i32
	lw     t0, 52(sp)
	lw     t1, 64(sp)
	sub    t2, t0, t1
	sw     t2, 68(sp)
	addi   t0, sp, 40
	; Store i32 to memory
	lw     t1, 68(sp)
	sw     t1, 0(t0)
	; local var: e
	; Load i32 from memory into $$10
	addi   t0, sp, 40
	lw     t1, 0(t0)
	sw     t1, 80(sp)
	lw     t0, 80(sp)
	sub    t1, zero, t0
	sw     t1, 84(sp)
	addi   t0, sp, 72
	; Store i32 to memory
	lw     t1, 84(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$12
	addi   t0, sp, 72
	lw     t1, 0(t0)
	sw     t1, 88(sp)
	lw     t2, 88(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 104
	ld     s0, 104(sp)
	; Restore return address (ra) from offset 96
	ld     ra, 96(sp)
	; Deallocate stack frame: 112 bytes
	addi   sp, sp, 112
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function