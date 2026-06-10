.section .text
; Function: divide
.globl divide
divide:
; --- Function Prologue ---
; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
; Save return address (ra) at offset 48
	sd     ra, 48(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; Save callee-saved register s3 at offset 32
	sd     s3, 32(sp)
; Save callee-saved register s4 at offset 40
	sd     s4, 40(sp)
; Save callee-saved register s0 at offset 56
	sd     s0, 56(sp)
; Set up frame pointer
	addi   s0, sp, 0
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, s0, 64
; Move parameter '$a' from register a0 to allocated register
	addiw  s2, a0, 0
; Move parameter '$b' from register a1 to allocated register
	addiw  s3, a1, 0
; --- End Parameter Spills ---
; Basic Block: entry
divide__entry:
; bind parameter: a
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; bind parameter: b
	addi   t0, sp, 8
; Store i32 to memory
	sw     s3, 0(t0)
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     s3, 0(t0)
; sdiv operation on i32
	div    s2, s2, s3
	addiw  s2, s2, 0
; Load i32 from memory into $$3
	addi   t0, sp, 0
	lw     s3, 0(t0)
; Load i32 from memory into $$4
	addi   t0, sp, 8
	lw     s4, 0(t0)
; mod operation on i32
	rem    s3, s3, s4
	addiw  s3, s3, 0
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
	addi   t0, sp, 20
; Store i32 to memory
	sw     s3, 0(t0)
	ld     a0, 16(sp)
; --- Function Epilogue ---
; Restore callee-saved register s0 from offset 56
	ld     s0, 56(sp)
; Restore callee-saved register s4 from offset 40
	ld     s4, 40(sp)
; Restore callee-saved register s3 from offset 32
	ld     s3, 32(sp)
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: test_tuple_destructuring
.globl test_tuple_destructuring
test_tuple_destructuring:
; --- Function Prologue ---
; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
; Save return address (ra) at offset 40
	sd     ra, 40(sp)
; Save callee-saved register s2 at offset 24
	sd     s2, 24(sp)
; Save callee-saved register s0 at offset 48
	sd     s0, 48(sp)
; Set up frame pointer
	addi   s0, sp, 0
; --- End Prologue ---
; Basic Block: entry
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
	sd     a0, 32(sp)
; --- End Function Call: divide ---
	addi   t0, sp, 0
; Store {quotient: i32, remainder: i32} to memory
	addi   t1, a0, 0
	sd     t1, 0(t0)
; Load i32 from memory into $$2
	addi   t0, sp, 0
	addi   t1, t0, 0
	lw     s2, 0(t1)
	addi   t0, sp, 8
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$3
	addi   t0, sp, 0
	addi   t1, t0, 4
	lw     s2, 0(t1)
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$4
	addi   t0, sp, 8
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s0 from offset 48
	ld     s0, 48(sp)
; Restore callee-saved register s2 from offset 24
	ld     s2, 24(sp)
; Restore return address (ra) from offset 40
	ld     ra, 40(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function