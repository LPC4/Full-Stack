.section .text
	; ========================================
	; Function: divide
	; ========================================
.globl divide
divide:
	; --- Function Prologue ---
	; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
	; Save return address (ra) at offset 56
	sd     ra, 56(sp)
	; Save callee-saved register s8 at offset 64
	sd     s0, 64(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 80
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
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 4(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$0
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	; sdiv operation on i32
	lw     t0, 24(sp)
	lw     t1, 28(sp)
	div    t2, t0, t1
	sw     t2, 32(sp)
	; Load i32 from memory into $$3
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	; Load i32 from memory into $$4
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	; mod operation on i32
	lw     t0, 36(sp)
	lw     t1, 40(sp)
	rem    t2, t0, t1
	sw     t2, 44(sp)
	addi   t0, sp, 48
	; Store i32 to memory
	lw     t1, 32(sp)
	sw     t1, 0(t0)
	addi   t0, sp, 52
	; Store i32 to memory
	lw     t1, 44(sp)
	sw     t1, 0(t0)
	lw     a0, 48(sp)
	lw     a1, 52(sp)
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 64
	ld     s0, 64(sp)
	; Restore return address (ra) from offset 56
	ld     ra, 56(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
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
	; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
	; Save return address (ra) at offset 56
	sd     ra, 56(sp)
	; Save callee-saved register s8 at offset 64
	sd     s0, 64(sp)
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
	addi   t0, sp, 24
	; Store i32 to memory
	lw     t1, 16(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$3
	addi   t0, sp, 8
	addi   t1, t0, 4
	lw     t2, 0(t1)
	sw     t2, 32(sp)
	addi   t0, sp, 40
	; Store i32 to memory
	lw     t1, 32(sp)
	sw     t1, 0(t0)
	; Load i32 from memory into $$4
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 48(sp)
	lw     t2, 48(sp)
	addi   a0, t2, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 64
	ld     s0, 64(sp)
	; Restore return address (ra) from offset 56
	ld     ra, 56(sp)
	; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function