.section .text
; Function: pointers
pointers:
; --- Function Prologue ---
; Allocate stack frame: 48 bytes
	addi   sp, sp, -48
; Save return address (ra) at offset 40
	sd     ra, 40(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; Save callee-saved register s3 at offset 32
	sd     s3, 32(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 48
; Move parameter '$val' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
pointers__entry:
; bind parameter: val
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; local var: ptr
	addi   a0, zero, 4
	addi   sp, sp, -16
	sd     a0, 0(sp)
	call malloc
	ld     a1, 0(sp)
	addi   sp, sp, 16
	addi   s2, a0, 0
	beq a0, zero, .Lheap_zero_done_0
	beq a1, zero, .Lheap_zero_done_0
	addi   t0, a0, 0
.Lheap_zero_0:
	sb     zero, 0(t0)
	addi   t0, t0, 1
	addi   a1, a1, -1
	bne a1, zero, .Lheap_zero_0
.Lheap_zero_done_0:
	addi   t0, sp, 8
; Store i32* to memory
	sd     s2, 0(t0)
; assignment
; Load i32* from memory into $$1
	addi   t0, sp, 8
; Load i32 from memory into $$2
	addi   t0, sp, 0
	lw     s3, 0(t0)
; Store i32 to memory
	sw     s3, 0(s2)
; local var: val_ref
	addi   t0, sp, 16
; Store i32* to memory
	addi   t1, sp, 0
	sd     t1, 0(t0)
; assignment
; Load i32* from memory into $$3
	addi   t0, sp, 16
	addi   s2, t1, 0
; Load i32* from memory into $$4
	addi   t0, sp, 8
	ld     s3, 0(t0)
; Load i32 from memory into $$5
	lw     s3, 0(s3)
; add operation on i32
	addi   t0, zero, 10
	add    s3, s3, t0
	addiw  s3, s3, 0
; Store i32 to memory
	sw     s3, 0(s2)
; Load i32* from memory into $$7
	addi   t0, sp, 8
	ld     s2, 0(t0)
; defer: captured call free with 1 args
; Load i32 from memory into $$8
	addi   t0, sp, 0
	lw     s3, 0(t0)
; executing deferred cleanup before return
; --- Function Call: free ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, free
; --- End Function Call: free ---
	addi   a0, s3, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 32
	ld     s3, 32(sp)
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 40
	ld     ra, 40(sp)
; Deallocate stack frame: 48 bytes
	addi   sp, sp, 48
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function