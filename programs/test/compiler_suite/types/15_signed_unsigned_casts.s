.section .text
	; ========================================
	; Function: print
	; ========================================
.globl print
print:
	; --- Function Prologue ---
	; Allocate stack frame: 32 bytes
	addi   sp, sp, -32
	; Save return address (ra) at offset 8
	sd     ra, 8(sp)
	; Save callee-saved register s8 at offset 16
	sd     s0, 16(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Function Parameter Spills ---
	addi   t0, s0, 32
	; Spill parameter '$value' from register a0 to stack slot 0
	sw     a0, 0(sp)
	; --- End Parameter Spills ---
	; --- Basic Block: entry ---
print__entry:
	; bind parameter: value
	addi   t0, sp, 4
	; Store i32 to memory
	lw     t1, 0(sp)
	sw     t1, 0(t0)
	; no body for function `print`
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 16
	ld     s0, 16(sp)
	; Restore return address (ra) from offset 8
	ld     ra, 8(sp)
	; Deallocate stack frame: 32 bytes
	addi   sp, sp, 32
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function

	; ========================================
	; Function: main
	; ========================================
.globl main
main:
	; --- Function Prologue ---
	; Allocate stack frame: 304 bytes
	addi   sp, sp, -304
	; Save return address (ra) at offset 288
	sd     ra, 288(sp)
	; Save callee-saved register s8 at offset 296
	sd     s0, 296(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Basic Block: entry ---
main__entry:
	; local var: signed_a
	addi   t0, sp, 0
	; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: signed_b
	addi   t0, sp, 4
	; Store i32 to memory
	addi   t1, zero, 3
	sw     t1, 0(t0)
	; local var: signed_result
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 12(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 4
	lw     t1, 0(t0)
	sw     t1, 16(sp)
	; sdiv operation on i32
	lw     t0, 12(sp)
	lw     t1, 16(sp)
	div    t2, t0, t1
	sw     t2, 20(sp)
	addi   t0, sp, 8
	; Store i32 to memory
	lw     t1, 20(sp)
	sw     t1, 0(t0)
	; local var: unsigned_a
	addi   t0, sp, 24
	; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: unsigned_b
	addi   t0, sp, 28
	; Store i32 to memory
	addi   t1, zero, 3
	sw     t1, 0(t0)
	; local var: unsigned_result
	; Load i32 from memory into $$3
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 36(sp)
	; Load i32 from memory into $$4
	addi   t0, sp, 28
	lw     t1, 0(t0)
	sw     t1, 40(sp)
	; div operation on i32
	lw     t0, 36(sp)
	lw     t1, 40(sp)
	div    t2, t0, t1
	sw     t2, 44(sp)
	addi   t0, sp, 32
	; Store i32 to memory
	lw     t1, 44(sp)
	sw     t1, 0(t0)
	; local var: signed_cmp
	addi   t0, zero, 5
	addi   t1, zero, 10
	slt    t2, t0, t1
	sb     t2, 49(sp)
	addi   t0, sp, 48
	; Store i1 to memory
	lb     t1, 49(sp)
	sb     t1, 0(t0)
	; local var: ua
	addi   t0, sp, 52
	; Store i32 to memory
	addi   t1, zero, 5
	sw     t1, 0(t0)
	; local var: ub
	addi   t0, sp, 56
	; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: unsigned_cmp
	; Load i32 from memory into $$7
	addi   t0, sp, 52
	lw     t1, 0(t0)
	sw     t1, 64(sp)
	; Load i32 from memory into $$8
	addi   t0, sp, 56
	lw     t1, 0(t0)
	sw     t1, 68(sp)
	lw     t0, 64(sp)
	lw     t1, 68(sp)
	sltu   t2, t0, t1
	sb     t2, 72(sp)
	addi   t0, sp, 60
	; Store i1 to memory
	lb     t1, 72(sp)
	sb     t1, 0(t0)
	; local var: small
	addi   t0, sp, 76
	; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
	; local var: big
	; Load i32 from memory into $$10
	addi   t0, sp, 76
	lw     t1, 0(t0)
	sw     t1, 88(sp)
	lw     t0, 88(sp)
	addi   t1, t0, 0
	sd     t1, 96(sp)
	addi   t0, sp, 80
	; Store i64 to memory
	ld     t1, 96(sp)
	sd     t1, 0(t0)
	; local var: large
	addi   t0, sp, 104
	; Store i64 to memory
	addi   t1, zero, 100
	sd     t1, 0(t0)
	; local var: small_again
	; Load i64 from memory into $$12
	addi   t0, sp, 104
	ld     t1, 0(t0)
	sd     t1, 120(sp)
	ld     t0, 120(sp)
	addi   t1, t0, 0
	sw     t1, 128(sp)
	addi   t0, sp, 112
	; Store i32 to memory
	lw     t1, 128(sp)
	sw     t1, 0(t0)
	; local var: int_val
	addi   t0, sp, 132
	; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
	; local var: float_val
	; Load i32 from memory into $$14
	addi   t0, sp, 132
	lw     t1, 0(t0)
	sw     t1, 144(sp)
	lw     t0, 144(sp)
	addi   t1, t0, 0
	fsd    ft6, 152(sp)
	addi   t0, sp, 136
	; Store f64 to memory
	fld    ft0, 152(sp)
	fsd    ft0, 0(t0)
	; local var: ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 168(sp)
	addi   t0, sp, 160
	; Store i32* to memory
	ld     t1, 168(sp)
	sd     t1, 0(t0)
	; assignment
	; Load i32* from memory into $$17
	addi   t0, sp, 160
	ld     t1, 0(t0)
	sd     t1, 176(sp)
	; Store value to dereferenced pointer (i32)
	ld     t0, 176(sp)
	; Store i32 to memory
	addi   t1, zero, 99
	sw     t1, 0(t0)
	; local var: heap_value
	; Load i32* from memory into $$18
	addi   t0, sp, 160
	ld     t1, 0(t0)
	sd     t1, 192(sp)
	; Load i32 from memory into $$19
	ld     t0, 192(sp)
	lw     t1, 0(t0)
	sw     t1, 200(sp)
	addi   t0, sp, 184
	; Store i32 to memory
	lw     t1, 200(sp)
	sw     t1, 0(t0)
	; Load i32* from memory into $$20
	addi   t0, sp, 160
	ld     t1, 0(t0)
	sd     t1, 208(sp)
	ld     t0, 208(sp)
	addi   a0, t0, 0
	call free
	; local var: int_ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 224(sp)
	addi   t0, sp, 216
	; Store i32* to memory
	ld     t1, 224(sp)
	sd     t1, 0(t0)
	; local var: byte_ptr
	; Load i32* from memory into $$22
	addi   t0, sp, 216
	ld     t1, 0(t0)
	sd     t1, 240(sp)
	ld     t0, 240(sp)
	addi   t1, t0, 0
	sd     t1, 248(sp)
	addi   t0, sp, 232
	; Store i8* to memory
	ld     t1, 248(sp)
	sd     t1, 0(t0)
	; Load i32* from memory into $$24
	addi   t0, sp, 216
	ld     t1, 0(t0)
	sd     t1, 256(sp)
	ld     t0, 256(sp)
	addi   a0, t0, 0
	call free
	; Load i32 from memory into $$25
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 264(sp)
	; Load i32 from memory into $$26
	addi   t0, sp, 32
	lw     t1, 0(t0)
	sw     t1, 268(sp)
	lw     t0, 268(sp)
	addi   t1, t0, 0
	sw     t1, 272(sp)
	; add operation on i32
	lw     t0, 264(sp)
	lw     t1, 272(sp)
	add    t2, t0, t1
	sw     t2, 276(sp)
	; Load i32 from memory into $$29
	addi   t0, sp, 184
	lw     t1, 0(t0)
	sw     t1, 280(sp)
	; add operation on i32
	lw     t0, 276(sp)
	lw     t1, 280(sp)
	add    t2, t0, t1
	sw     t2, 284(sp)
	lw     t3, 284(sp)
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 296
	ld     s0, 296(sp)
	; Restore return address (ra) from offset 288
	ld     ra, 288(sp)
	; Deallocate stack frame: 304 bytes
	addi   sp, sp, 304
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function