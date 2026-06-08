.section .text
	; ========================================
	; Function: main
	; ========================================
.globl main
main:
	; --- Function Prologue ---
	; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
	; Save return address (ra) at offset 64
	sd     ra, 64(sp)
	; Save callee-saved register s8 at offset 72
	sd     s0, 72(sp)
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
	sw     t1, 40(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 48(sp)
	; add operation on i32
	lw     t0, 40(sp)
	lw     t1, 48(sp)
	add    t2, t0, t1
	sw     t2, 40(sp)
	; mul operation on i32
	lw     t0, 40(sp)
	addi   t1, zero, 2
	mul    t2, t0, t1
	sw     t2, 40(sp)
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 40(sp)
	sw     t1, 0(t0)
	; local var: d
	; Load i32 from memory into $$4
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	; sdiv operation on i32
	lw     t0, 40(sp)
	addi   t1, zero, 5
	div    t2, t0, t1
	sw     t2, 40(sp)
	; Load i32 from memory into $$6
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 48(sp)
	; Load i32 from memory into $$7
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 56(sp)
	; mod operation on i32
	lw     t0, 48(sp)
	lw     t1, 56(sp)
	rem    t2, t0, t1
	sw     t2, 48(sp)
	; sub operation on i32
	lw     t0, 40(sp)
	lw     t1, 48(sp)
	sub    t2, t0, t1
	sw     t2, 40(sp)
	addi   t0, sp, 24
	; Store i32 to memory
	lw     t1, 40(sp)
	sw     t1, 0(t0)
	; local var: e
	; Load i32 from memory into $$10
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	lw     t0, 40(sp)
	sub    t1, zero, t0
	sw     t1, 40(sp)
	addi   t0, sp, 32
	; Store i32 to memory
	lw     t1, 40(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$12
	addi   t0, sp, 32
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	lw     t2, 40(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 72
	ld     s0, 72(sp)
	; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function