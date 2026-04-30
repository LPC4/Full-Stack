.section .text
	; ========================================
	; Function: divide
	; ========================================
.globl divide
divide:
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
	; --- Function Parameter Spills ---
	addi   t0, s0, 64
	; Spill parameter '$a' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; Spill parameter '$b' from register a1 to stack slot 4
	sw     a1, 4(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
divide__entry:
	; bind parameter: a
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; bind parameter: b
	addi   t0, sp, 12
	; Store i32 to memory
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$0
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 20(sp)
	; sdiv operation on i32
	lw     t0, 16(sp)
	lw     t1, 20(sp)
	div    t2, t0, t1
	sw     t2, 24(sp)
	; Load i32 from memory into $$3
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	; Load i32 from memory into $$4
	addi   t0, sp, 12
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	; mod operation on i32
	lw     t0, 28(sp)
	lw     t1, 32(sp)
	rem    t2, t0, t1
	sw     t2, 36(sp)
	addi   t0, sp, 40
	; Store i32 to memory
	lw     t1, 24(sp)
	sw     t1, 0(t0)
	addi   t0, sp, 44
	; Store i32 to memory
	lw     t1, 36(sp)
	sw     t1, 0(t0)
	lw     a0, 40(sp)
	lw     a1, 44(sp)
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

	; ========================================
	; Function: test_tuple_destructuring
	; ========================================
.globl test_tuple_destructuring
test_tuple_destructuring:
	; --- Function Prologue ---
	; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
	; Save return address (ra) at offset 40
	sd     ra, 40(sp)
	; Save callee-saved register s8 at offset 48
	sd     s0, 48(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Basic Block: entry ---
test_tuple_destructuring__entry:
	; assignment
	; --- Function Call: divide ---
	; Passing 2 arguments
	addi   t0, zero, 10
	addi   a0, t0, 0
	addi   t1, zero, 3
	addi   a1, t1, 0
	jal ra, divide
	; Unpacking small aggregate return from a0/a1
	sw     a0, 0(sp)
	sw     a1, 4(sp)
	; --- End Function Call: divide ---
	addi   t0, sp, 8
	; Store {quotient: i32, remainder: i32} to memory
	ld     t1, 0(sp)
	sd     t1, 0(t0)
	; Load i32 from memory into $$2
	addi   t0, sp, 8
	addi   t1, t0, 0
	lw     t2, 0(t1)
	sw     t2, 16(sp)
	addi   t0, sp, 20
	; Store i32 to memory
	lw     t1, 16(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$3
	addi   t0, sp, 8
	addi   t1, t0, 4
	lw     t2, 0(t1)
	sw     t2, 24(sp)
	addi   t0, sp, 28
	; Store i32 to memory
	lw     t1, 24(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$4
	addi   t0, sp, 20
	lw     t1, 0(t0)
	sw     t1, 32(sp)
	lw     t2, 32(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 48
	ld     s0, 48(sp)
	; Restore return address (ra) from offset 40
	ld     ra, 40(sp)
	; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function