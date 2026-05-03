.section .text
	; ========================================
	; Function: main
	; ========================================
.globl main
main:
	; --- Function Prologue ---
	; Allocate stack frame: 384 bytes
	addi   sp, sp, -384
	; Save return address (ra) at offset 360
	sd     ra, 360(sp)
	; Save callee-saved register s8 at offset 368
	sd     s0, 368(sp)
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
	addi   t0, sp, 8
	; Store i32 to memory
	addi   t1, zero, 3
	sw     t1, 0(t0)
	; local var: signed_result
	; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     t1, 0(t0)
	sw     t1, 24(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 28(sp)
	; sdiv operation on i32
	lw     t0, 24(sp)
	lw     t1, 28(sp)
	div    t2, t0, t1
	sw     t2, 32(sp)
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 32(sp)
	sw     t1, 0(t0)
	; local var: unsigned_a
	addi   t0, sp, 40
	; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: unsigned_b
	addi   t0, sp, 48
	; Store i32 to memory
	addi   t1, zero, 3
	sw     t1, 0(t0)
	; local var: unsigned_result
	; Load i32 from memory into $$3
	addi   t0, sp, 40
	lw     t1, 0(t0)
	sw     t1, 64(sp)
	; Load i32 from memory into $$4
	addi   t0, sp, 48
	lw     t1, 0(t0)
	sw     t1, 68(sp)
	; div operation on i32
	lw     t0, 64(sp)
	lw     t1, 68(sp)
	div    t2, t0, t1
	sw     t2, 72(sp)
	addi   t0, sp, 56
	; Store i32 to memory
	lw     t1, 72(sp)
	sw     t1, 0(t0)
	; local var: signed_cmp
	addi   t0, zero, 5
	addi   t1, zero, 10
	slt    t2, t0, t1
	sb     t2, 88(sp)
	addi   t0, sp, 80
	; Store i1 to memory
	lb     t1, 88(sp)
	sb     t1, 0(t0)
	; local var: ua
	addi   t0, sp, 96
	; Store i32 to memory
	addi   t1, zero, 5
	sw     t1, 0(t0)
	; local var: ub
	addi   t0, sp, 104
	; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: unsigned_cmp
	; Load i32 from memory into $$7
	addi   t0, sp, 96
	lw     t1, 0(t0)
	sw     t1, 120(sp)
	; Load i32 from memory into $$8
	addi   t0, sp, 104
	lw     t1, 0(t0)
	sw     t1, 124(sp)
	lw     t0, 120(sp)
	lw     t1, 124(sp)
	sltu   t2, t0, t1
	sb     t2, 128(sp)
	addi   t0, sp, 112
	; Store i1 to memory
	lb     t1, 128(sp)
	sb     t1, 0(t0)
	; local var: small
	addi   t0, sp, 136
	; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
	; local var: big
	; Load i32 from memory into $$10
	addi   t0, sp, 136
	lw     t1, 0(t0)
	sw     t1, 152(sp)
	lw     t0, 152(sp)
	addi   t1, t0, 0
	sd     t1, 160(sp)
	addi   t0, sp, 144
	; Store i64 to memory
	ld     t1, 160(sp)
	sd     t1, 0(t0)
	; local var: large
	addi   t0, sp, 168
	; Store i64 to memory
	addi   t1, zero, 100
	sd     t1, 0(t0)
	; local var: small_again
	; Load i64 from memory into $$12
	addi   t0, sp, 168
	ld     t1, 0(t0)
	sd     t1, 184(sp)
	ld     t0, 184(sp)
	addi   t1, t0, 0
	sw     t1, 192(sp)
	addi   t0, sp, 176
	; Store i32 to memory
	lw     t1, 192(sp)
	sw     t1, 0(t0)
	; local var: int_val
	addi   t0, sp, 200
	; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
	; local var: float_val
	; Load i32 from memory into $$14
	addi   t0, sp, 200
	lw     t1, 0(t0)
	sw     t1, 216(sp)
	lw     t0, 216(sp)
	addi   t1, t0, 0
	fsd    ft6, 224(sp)
	addi   t0, sp, 208
	; Store f64 to memory
	fld    ft0, 224(sp)
	fsd    ft0, 0(t0)
	; local var: ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 240(sp)
	addi   t0, sp, 232
	; Store i32* to memory
	ld     t1, 240(sp)
	sd     t1, 0(t0)
	; assignment
	; Load i32* from memory into $$17
	addi   t0, sp, 232
	ld     t1, 0(t0)
	sd     t1, 248(sp)
	ld     t0, 248(sp)
	; Store i32 to memory
	addi   t1, zero, 99
	sw     t1, 0(t0)
	; local var: heap_value
	; Load i32* from memory into $$18
	addi   t0, sp, 232
	ld     t1, 0(t0)
	sd     t1, 264(sp)
	; Load i32 from memory into $$19
	ld     t0, 264(sp)
	lw     t1, 0(t0)
	sw     t1, 272(sp)
	addi   t0, sp, 256
	; Store i32 to memory
	lw     t1, 272(sp)
	sw     t1, 0(t0)
	; Load i32* from memory into $$20
	addi   t0, sp, 232
	ld     t1, 0(t0)
	sd     t1, 280(sp)
	ld     t0, 280(sp)
	addi   a0, t0, 0
	call free
	; local var: int_ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 296(sp)
	addi   t0, sp, 288
	; Store i32* to memory
	ld     t1, 296(sp)
	sd     t1, 0(t0)
	; local var: byte_ptr
	; Load i32* from memory into $$22
	addi   t0, sp, 288
	ld     t1, 0(t0)
	sd     t1, 312(sp)
	ld     t0, 312(sp)
	addi   t1, t0, 0
	sd     t1, 320(sp)
	addi   t0, sp, 304
	; Store i8* to memory
	ld     t1, 320(sp)
	sd     t1, 0(t0)
	; Load i32* from memory into $$24
	addi   t0, sp, 288
	ld     t1, 0(t0)
	sd     t1, 328(sp)
	ld     t0, 328(sp)
	addi   a0, t0, 0
	call free
	; Load i32 from memory into $$25
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 336(sp)
	; Load i32 from memory into $$26
	addi   t0, sp, 56
	lw     t1, 0(t0)
	sw     t1, 340(sp)
	lw     t0, 340(sp)
	addi   t1, t0, 0
	sw     t1, 344(sp)
	; add operation on i32
	lw     t0, 336(sp)
	lw     t1, 344(sp)
	add    t2, t0, t1
	sw     t2, 348(sp)
	; Load i32 from memory into $$29
	addi   t0, sp, 256
	lw     t1, 0(t0)
	sw     t1, 352(sp)
	; add operation on i32
	lw     t0, 348(sp)
	lw     t1, 352(sp)
	add    t2, t0, t1
	sw     t2, 356(sp)
	lw     t3, 356(sp)
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 368
	ld     s0, 368(sp)
	; Restore return address (ra) from offset 360
	ld     ra, 360(sp)
	; Deallocate stack frame: 384 bytes
	addi   sp, sp, 384
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function