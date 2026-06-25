.section .text
; Function: main
.globl main
main:
; --- Function Prologue ---
; Allocate stack frame: 192 bytes
	addi   sp, sp, -192
; Save return address (ra) at offset 184
	sd     ra, 184(sp)
; Save callee-saved register s2 at offset 168
	sd     s2, 168(sp)
; Save callee-saved register s3 at offset 176
	sd     s3, 176(sp)
; --- End Prologue ---
; Basic Block: entry
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
	lw     s2, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     s3, 0(t0)
; sdiv operation on i32
	div    s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
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
	lw     s2, 0(t0)
; Load i32 from memory into $$4
	addi   t0, sp, 32
	lw     s3, 0(t0)
; udiv operation on i32
	divu   s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 40
; Store i32 to memory
	sw     s2, 0(t0)
; local var: signed_cmp
	addi   t0, zero, 5
	addi   t1, zero, 10
	slt    s2, t0, t1
	addi   t0, sp, 48
; Store i1 to memory
	sb     s2, 0(t0)
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
	lw     s2, 0(t0)
; Load i32 from memory into $$8
	addi   t0, sp, 64
	lw     s3, 0(t0)
	sltu   s2, s2, s3
	addi   t0, sp, 72
; Store i1 to memory
	sb     s2, 0(t0)
; local var: small
	addi   t0, sp, 80
; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
; local var: big
; Load i32 from memory into $$10
	addi   t0, sp, 80
	lw     s2, 0(t0)
	addi   t0, sp, 88
; Store i64 to memory
	sd     s2, 0(t0)
; local var: large
	addi   t0, sp, 96
; Store i64 to memory
	addi   t1, zero, 100
	sd     t1, 0(t0)
; local var: small_again
; Load i64 from memory into $$12
	addi   t0, sp, 96
	addi   s2, t1, 0
	addiw  s2, s2, 0
	addi   t0, sp, 104
; Store i32 to memory
	sw     s2, 0(t0)
; local var: int_val
	addi   t0, sp, 112
; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
; local var: float_val
; Load i32 from memory into $$14
	addi   t0, sp, 112
	lw     s2, 0(t0)
	fcvt.d.w ft0, s2
	fsd    ft0, 160(sp)
	addi   t0, sp, 120
; Store f64 to memory
	fld    ft1, 160(sp)
	fsd    ft1, 0(t0)
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
	addi   t0, sp, 128
; Store i32* to memory
	sd     s2, 0(t0)
; assignment
; Load i32* from memory into $$17
	addi   t0, sp, 128
; Store i32 to memory
	addi   t0, zero, 99
	sw     t0, 0(s2)
; local var: heap_value
; Load i32* from memory into $$18
	addi   t0, sp, 128
	ld     s2, 0(t0)
; Load i32 from memory into $$19
	lw     s2, 0(s2)
	addi   t0, sp, 136
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32* from memory into $$20
	addi   t0, sp, 128
	ld     s2, 0(t0)
	addi   a0, s2, 0
	call free
; local var: int_ptr
	addi   a0, zero, 4
	addi   sp, sp, -16
	sd     a0, 0(sp)
	call malloc
	ld     a1, 0(sp)
	addi   sp, sp, 16
	addi   s2, a0, 0
	beq a0, zero, .Lheap_zero_done_1
	beq a1, zero, .Lheap_zero_done_1
	addi   t0, a0, 0
.Lheap_zero_1:
	sb     zero, 0(t0)
	addi   t0, t0, 1
	addi   a1, a1, -1
	bne a1, zero, .Lheap_zero_1
.Lheap_zero_done_1:
	addi   t0, sp, 144
; Store i32* to memory
	sd     s2, 0(t0)
; local var: byte_ptr
; Load i32* from memory into $$22
	addi   t0, sp, 144
	addi   t0, sp, 152
; Store i8* to memory
	sd     s2, 0(t0)
; Load i32* from memory into $$24
	addi   t0, sp, 144
	addi   a0, s2, 0
	call free
; Load i32 from memory into $$25
	addi   t0, sp, 16
	lw     s2, 0(t0)
; Load i32 from memory into $$26
	addi   t0, sp, 40
	lw     s3, 0(t0)
	slli  s3, s3, 32
	srli  s3, s3, 32
	addiw  s3, s3, 0
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
; Load i32 from memory into $$29
	addi   t0, sp, 136
	lw     s3, 0(t0)
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 176
	ld     s3, 176(sp)
; Restore callee-saved register s2 from offset 168
	ld     s2, 168(sp)
; Restore return address (ra) from offset 184
	ld     ra, 184(sp)
; Deallocate stack frame: 192 bytes
	addi   sp, sp, 192
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function