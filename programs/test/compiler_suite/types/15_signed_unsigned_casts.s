.section .text
	; ========================================
	; Function: main
	; ========================================
.globl main
main:
	; --- Function Prologue ---
	; Allocate stack frame: 352 bytes
	addi   sp, sp, -352
	; Save return address (ra) at offset 336
	sd     ra, 336(sp)
	; Save callee-saved register s8 at offset 344
	sd     s0, 344(sp)
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
	sw     t1, 160(sp)
	; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     t1, 0(t0)
	sw     t1, 164(sp)
	; sdiv operation on i32
	lw     t0, 160(sp)
	lw     t1, 164(sp)
	div    t2, t0, t1
	sw     t2, 168(sp)
	addi   t0, sp, 16
	; Store i32 to memory
	lw     t1, 168(sp)
	sw     t1, 0(t0)
	; local var: unsigned_a
	addi   t0, sp, 24
	; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: unsigned_b
	addi   t0, sp, 32
	; Store i32 to memory
	addi   t1, zero, 3
	sw     t1, 0(t0)
	; local var: unsigned_result
	; Load i32 from memory into $$3
	addi   t0, sp, 24
	lw     t1, 0(t0)
	sw     t1, 172(sp)
	; Load i32 from memory into $$4
	addi   t0, sp, 32
	lw     t1, 0(t0)
	sw     t1, 176(sp)
	; div operation on i32
	lw     t0, 172(sp)
	lw     t1, 176(sp)
	div    t2, t0, t1
	sw     t2, 180(sp)
	addi   t0, sp, 40
	; Store i32 to memory
	lw     t1, 180(sp)
	sw     t1, 0(t0)
	; local var: signed_cmp
	addi   t0, zero, 5
	addi   t1, zero, 10
	slt    t2, t0, t1
	sb     t2, 184(sp)
	addi   t0, sp, 48
	; Store i1 to memory
	lb     t1, 184(sp)
	sb     t1, 0(t0)
	; local var: ua
	addi   t0, sp, 56
	; Store i32 to memory
	addi   t1, zero, 5
	sw     t1, 0(t0)
	; local var: ub
	addi   t0, sp, 64
	; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
	; local var: unsigned_cmp
	; Load i32 from memory into $$7
	addi   t0, sp, 56
	lw     t1, 0(t0)
	sw     t1, 188(sp)
	; Load i32 from memory into $$8
	addi   t0, sp, 64
	lw     t1, 0(t0)
	sw     t1, 192(sp)
	lw     t0, 188(sp)
	lw     t1, 192(sp)
	sltu   t2, t0, t1
	sb     t2, 196(sp)
	addi   t0, sp, 72
	; Store i1 to memory
	lb     t1, 196(sp)
	sb     t1, 0(t0)
	; local var: small
	addi   t0, sp, 80
	; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
	; local var: big
	; Load i32 from memory into $$10
	addi   t0, sp, 80
	lw     t1, 0(t0)
	sw     t1, 200(sp)
	lw     t0, 200(sp)
	addi   t1, t0, 0
	sd     t1, 208(sp)
	addi   t0, sp, 88
	; Store i64 to memory
	ld     t1, 208(sp)
	sd     t1, 0(t0)
	; local var: large
	addi   t0, sp, 96
	; Store i64 to memory
	addi   t1, zero, 100
	sd     t1, 0(t0)
	; local var: small_again
	; Load i64 from memory into $$12
	addi   t0, sp, 96
	ld     t1, 0(t0)
	sd     t1, 216(sp)
	ld     t0, 216(sp)
	addi   t1, t0, 0
	sw     t1, 224(sp)
	addi   t0, sp, 104
	; Store i32 to memory
	lw     t1, 224(sp)
	sw     t1, 0(t0)
	; local var: int_val
	addi   t0, sp, 112
	; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
	; local var: float_val
	; Load i32 from memory into $$14
	addi   t0, sp, 112
	lw     t1, 0(t0)
	sw     t1, 228(sp)
	lw     t0, 228(sp)
	addi   t1, t0, 0
	fsd    ft6, 232(sp)
	addi   t0, sp, 120
	; Store f64 to memory
	fld    ft0, 232(sp)
	fsd    ft0, 0(t0)
	; local var: ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 240(sp)
	addi   t0, sp, 128
	; Store i32* to memory
	ld     t1, 240(sp)
	sd     t1, 0(t0)
	; assignment
	; Load i32* from memory into $$17
	addi   t0, sp, 128
	ld     t1, 0(t0)
	sd     t1, 248(sp)
	ld     t0, 248(sp)
	; Store i32 to memory
	addi   t1, zero, 99
	sw     t1, 0(t0)
	; local var: heap_value
	; Load i32* from memory into $$18
	addi   t0, sp, 128
	ld     t1, 0(t0)
	sd     t1, 256(sp)
	; Load i32 from memory into $$19
	ld     t0, 256(sp)
	lw     t1, 0(t0)
	sw     t1, 264(sp)
	addi   t0, sp, 136
	; Store i32 to memory
	lw     t1, 264(sp)
	sw     t1, 0(t0)
	; Load i32* from memory into $$20
	addi   t0, sp, 128
	ld     t1, 0(t0)
	sd     t1, 272(sp)
	ld     t0, 272(sp)
	addi   a0, t0, 0
	call free
	; local var: int_ptr
	addi   a0, zero, 4
	call malloc
	sd     a0, 280(sp)
	addi   t0, sp, 144
	; Store i32* to memory
	ld     t1, 280(sp)
	sd     t1, 0(t0)
	; local var: byte_ptr
	; Load i32* from memory into $$22
	addi   t0, sp, 144
	ld     t1, 0(t0)
	sd     t1, 288(sp)
	ld     t0, 288(sp)
	addi   t1, t0, 0
	sd     t1, 296(sp)
	addi   t0, sp, 152
	; Store i8* to memory
	ld     t1, 296(sp)
	sd     t1, 0(t0)
	; Load i32* from memory into $$24
	addi   t0, sp, 144
	ld     t1, 0(t0)
	sd     t1, 304(sp)
	ld     t0, 304(sp)
	addi   a0, t0, 0
	call free
	; Load i32 from memory into $$25
	addi   t0, sp, 16
	lw     t1, 0(t0)
	sw     t1, 312(sp)
	; Load i32 from memory into $$26
	addi   t0, sp, 40
	lw     t1, 0(t0)
	sw     t1, 316(sp)
	lw     t0, 316(sp)
	addi   t1, t0, 0
	sw     t1, 320(sp)
	; add operation on i32
	lw     t0, 312(sp)
	lw     t1, 320(sp)
	add    t2, t0, t1
	sw     t2, 324(sp)
	; Load i32 from memory into $$29
	addi   t0, sp, 136
	lw     t1, 0(t0)
	sw     t1, 328(sp)
	; add operation on i32
	lw     t0, 324(sp)
	lw     t1, 328(sp)
	add    t2, t0, t1
	sw     t2, 332(sp)
	lw     t3, 332(sp)
	addi   a0, t3, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 344
	ld     s0, 344(sp)
	; Restore return address (ra) from offset 336
	ld     ra, 336(sp)
	; Deallocate stack frame: 352 bytes
	addi   sp, sp, 352
	; Return to caller
	jalr   zero, 0(ra)
	; --- End Epilogue ---
	; End of function