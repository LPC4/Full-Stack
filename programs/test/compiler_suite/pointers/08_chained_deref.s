.section .text
	; ========================================
	; Function: chained_deref_assign
	; ========================================
.globl chained_deref_assign
chained_deref_assign:
	; --- Function Prologue ---
	; Allocate stack frame: 96 bytes
	addi   sp, sp, -96
	; Save return address (ra) at offset 80
	sd     ra, 80(sp)
	; Save callee-saved register s8 at offset 88
	sd     s0, 88(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 96
	; Spill parameter '$val' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
chained_deref_assign__entry:
	; bind parameter: val
	addi   t0, sp, 4
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; local var: ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 16(sp)
	addi   t0, sp, 8
	; Store i32* to memory
	ld     t1, 16(sp)
	sd     t1, 0(t0)
	; local var: pp
	addi   t0, sp, 24
	; Store i32** to memory
	addi   t1, sp, 8
	sd     t1, 0(t0)
	; assignment
	; Load i32 from memory into $$1
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	; add operation on i32
	lw     t0, 32(sp)
	addi   t1, zero, 1
	add    t2, t0, t1
	sw     t2, 36(sp)
	; Load i32** from memory into $$3
	addi   t0, sp, 24
	ld     t1, 0(t0)
	sd     t1, 40(sp)
	; Load i32* from memory into $$4
	ld     t0, 40(sp)
	ld     t1, 0(t0)
	sd     t1, 48(sp)
	ld     t0, 48(sp)
	; Store i32 to memory
	lw     t1, 36(sp)
	sw     t1, 0(t0)
	; Load i32* from memory into $$5
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 56(sp)
	; defer: captured call free with 1 args
	; Load i32* from memory into $$6
	addi   t0, sp, 8
	ld     t1, 0(t0)
	sd     t1, 64(sp)
	; Load i32 from memory into $$7
	ld     t0, 64(sp)
	lw     t1, 0(t0)
	sw     t1, 72(sp)
	; executing deferred cleanup before return
	; --- Function Call: free ---
	; Passing 1 arguments
	ld     t0, 56(sp)
	addi   a0, t0, 0
	jal ra, free
	; --- End Function Call: free ---
	lw     t1, 72(sp)
	addi   a0, t1, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 88
	ld     s0, 88(sp)
	; Restore return address (ra) from offset 80
	ld     ra, 80(sp)
	; Deallocate stack frame: 96 bytes
	addi   sp, sp, 96
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function