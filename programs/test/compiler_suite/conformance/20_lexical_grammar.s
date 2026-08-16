.section .text
; Function: select
select:
; --- Function Prologue ---
; Allocate stack frame: 80 bytes
	addi   sp, sp, -80
; Save return address (ra) at offset 64
	sd     ra, 64(sp)
; Save callee-saved register s2 at offset 48
	sd     s2, 48(sp)
; Save callee-saved register s3 at offset 56
	sd     s3, 56(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 80
; Copy aggregate parameter '$choice' from indirect ABI argument
	ld     t1, 0(a0)
	sd     t1, 32(sp)
	ld     t2, 8(a0)
	sd     t2, 40(sp)
; --- End Parameter Spills ---
; Basic Block: entry
select__entry:
	addi   t0, sp, 0
; Store Choice to memory
	ld     t1, 32(sp)
	sd     t1, 0(t0)
	ld     t2, 40(sp)
	sd     t2, 8(t0)
; Load i64 from memory into $$0
	addi   t0, sp, 0
	addi   t1, t0, 0
	ld     s2, 0(t1)
	addi   t0, zero, 0
	sub    t1, s2, t0
	sltiu  s3, t1, 1
	bne s3, zero, select__match_arm_1
	j select__match_next_2
; Basic Block: match_arm_1
select__match_arm_1:
; Load i32 from memory into $$2
	addi   t0, sp, 0
	addi   t1, t0, 8
	lw     s2, 0(t1)
	addi   t0, sp, 24
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$3
	addi   t0, sp, 24
	lw     s2, 0(t0)
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
	j select__match_merge_0
; Basic Block: match_next_2
select__match_next_2:
	addi   t0, zero, 1
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, select__match_arm_3
	j select__match_next_4
; Basic Block: match_arm_3
select__match_arm_3:
	addi   t0, sp, 16
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	j select__match_merge_0
; Basic Block: match_next_4
select__match_next_4:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: match_merge_0
select__match_merge_0:
; Load i32 from memory into $$5
	addi   t0, sp, 16
	lw     s2, 0(t0)
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 56
	ld     s3, 56(sp)
; Restore callee-saved register s2 from offset 48
	ld     s2, 48(sp)
; Restore return address (ra) from offset 64
	ld     ra, 64(sp)
; Deallocate stack frame: 80 bytes
	addi   sp, sp, 80
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 208 bytes
	addi   sp, sp, -208
; Save return address (ra) at offset 200
	sd     ra, 200(sp)
; Save callee-saved register s2 at offset 184
	sd     s2, 184(sp)
; Save callee-saved register s3 at offset 192
	sd     s3, 192(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: alias_value
	addi   t0, sp, 0
; Store Word to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
; local var: binary_value
	addi   t0, sp, 8
; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
; local var: decimal_value
	addi   t0, sp, 16
; Store i32 to memory
	addi   t1, zero, 42
	sw     t1, 0(t0)
; local var: escaped
	addi   t0, sp, 24
; Store i32 to memory
	addi   t1, zero, 10
	sw     t1, 0(t0)
; local var: text
	addi   t0, sp, 48
; Store i8* to memory
	la t1, str_0
	sd     t1, 0(t0)
	addi   t0, sp, 56
; Store i64 to memory
	addi   t1, zero, 3
	sd     t1, 0(t0)
	addi   t0, sp, 32
; Store i8[] to memory
	ld     t1, 48(sp)
	sd     t1, 0(t0)
	ld     t2, 56(sp)
	sd     t2, 8(t0)
; local var: ratio
	addi   t0, sp, 64
; Store f64 to memory
	lui    t1, 0x3ff80
	slli  t1, t1, 32
	lui    t2, 0x0
	or     t1, t1, t2
	fmv.d.x ft0, t1
	fsd    ft0, 0(t0)
; Load i32 from memory into $$1
	addi   t0, sp, 8
	lw     s2, 0(t0)
	addi   t0, zero, 42
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_5
	j main__if_merge_7
; Basic Block: if_then_5
main__if_then_5:
	addi   t2, zero, 1
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 200
	ld     ra, 200(sp)
; Deallocate stack frame: 208 bytes
	addi   sp, sp, 208
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_7
main__if_merge_7:
; Load i32 from memory into $$3
	addi   t0, sp, 16
	lw     s2, 0(t0)
	addi   t0, zero, 42
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_8
	j main__if_merge_10
; Basic Block: if_then_8
main__if_then_8:
	addi   t2, zero, 2
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 200
	ld     ra, 200(sp)
; Deallocate stack frame: 208 bytes
	addi   sp, sp, 208
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_10
main__if_merge_10:
; Load i32 from memory into $$5
	addi   t0, sp, 24
	lw     s2, 0(t0)
	addi   t0, zero, 10
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_11
	j main__if_merge_13
; Basic Block: if_then_11
main__if_then_11:
	addi   t2, zero, 3
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 200
	ld     ra, 200(sp)
; Deallocate stack frame: 208 bytes
	addi   sp, sp, 208
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_13
main__if_merge_13:
; Access field 'len' at offset 8
; Load i64 from memory into $$7
	addi   t0, sp, 32
	addi   t1, t0, 8
	ld     s2, 0(t1)
	addi   t0, zero, 3
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_14
	j main__if_merge_16
; Basic Block: if_then_14
main__if_then_14:
	addi   t2, zero, 4
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 200
	ld     ra, 200(sp)
; Deallocate stack frame: 208 bytes
	addi   sp, sp, 208
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_16
main__if_merge_16:
; local var: pair
; Load i32 from memory into $$10
	addi   t0, sp, 8
	lw     s2, 0(t0)
	addi   t0, sp, 80
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$11
	addi   t0, sp, 16
	lw     s2, 0(t0)
	addi   t0, sp, 84
; Store i32 to memory
	sw     s2, 0(t0)
	addi   t0, sp, 72
; Store Pair to memory
	ld     t1, 80(sp)
	sd     t1, 0(t0)
; local var: values
; Access field 'left' at offset 0
; Load i32 from memory into $$12
	addi   t0, sp, 72
	addi   t1, t0, 0
	lw     s2, 0(t1)
; Access field 'right' at offset 4
; Load i32 from memory into $$13
	addi   t0, sp, 72
	addi   t1, t0, 4
	lw     s3, 0(t1)
	addi   t0, sp, 96
; Store i32 to memory
	sw     s2, 0(t0)
	addi   t0, sp, 100
; Store i32 to memory
	sw     s3, 0(t0)
	addi   t0, sp, 88
; Store i32[2] to memory
	ld     t1, 96(sp)
	sd     t1, 0(t0)
; local var: total
	addi   t0, sp, 104
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	addi   t0, sp, 88
	addi   t1, zero, 0
	addi   t3, zero, 4
	mul    t2, t1, t3
	add    s2, t0, t2
; inferred local var: __for_ptr_0
	addi   t0, sp, 112
; Store i32* to memory
	sd     s2, 0(t0)
; inferred local var: __for_i_0
	addi   t0, sp, 120
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	j main__while_cond_17
; Basic Block: while_cond_17
main__while_cond_17:
; Load i32 from memory into $$16
	addi   t0, sp, 120
	lw     s2, 0(t0)
	addi   t0, zero, 2
	slt    s2, s2, t0
	bne s2, zero, main__while_body_18
	j main__while_exit_19
; Basic Block: while_body_18
main__while_body_18:
; Load i32 from memory into $$18
	addi   t0, sp, 120
	lw     s2, 0(t0)
; Load i32* from memory into $$19
	addi   t0, sp, 112
	ld     s3, 0(t0)
	addi   t1, zero, 4
	mul    t0, s2, t1
	add    s2, s3, t0
; Load i32 from memory into $$21
	lw     s2, 0(s2)
; inferred local var: value
	addi   t0, sp, 128
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$22
	addi   t0, sp, 104
	lw     s2, 0(t0)
; Load i32 from memory into $$23
	addi   t0, sp, 128
	lw     s3, 0(t0)
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 104
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$25
	addi   t0, sp, 120
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 120
; Store i32 to memory
	sw     s2, 0(t0)
	j main__while_cond_17
; Basic Block: while_exit_19
main__while_exit_19:
	j main__while_cond_20
; Basic Block: while_cond_20
main__while_cond_20:
; Load i32 from memory into $$27
	addi   t0, sp, 104
	lw     s2, 0(t0)
	addi   t0, zero, 42
	slt    s2, t0, s2
	bne s2, zero, main__while_body_21
	j main__while_exit_22
; Basic Block: while_body_21
main__while_body_21:
; Load i32 from memory into $$29
	addi   t0, sp, 104
	lw     s2, 0(t0)
; sub operation on i32
	addi   t0, zero, 42
	sub    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 104
; Store i32 to memory
	sw     s2, 0(t0)
	j main__while_cond_20
; Basic Block: while_exit_22
main__while_exit_22:
; local var: choice
	addi   t0, sp, 152
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
; Load i32 from memory into $$32
	addi   t0, sp, 104
	lw     s2, 0(t0)
	addi   t0, sp, 160
; Store i32 to memory
	sw     s2, 0(t0)
	addi   t0, sp, 136
; Store Choice to memory
	sd     t1, 0(t0)
	ld     t2, 160(sp)
	sd     t2, 8(t0)
; Load Choice from memory into $$33
	addi   t0, sp, 136
	sd     t1, 168(sp)
	sd     t2, 176(sp)
; --- Function Call: select ---
; Passing 1 arguments
	addi   t0, sp, 168
	addi   a0, t0, 0
	jal ra, select
	addiw  s2, a0, 0
; --- End Function Call: select ---
	addi   t0, zero, 42
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_23
	j main__if_merge_25
; Basic Block: if_then_23
main__if_then_23:
	addi   t2, zero, 6
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 200
	ld     ra, 200(sp)
; Deallocate stack frame: 208 bytes
	addi   sp, sp, 208
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_25
main__if_merge_25:
	addi   t3, zero, 42
	addi   a0, t3, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 200
	ld     ra, 200(sp)
; Deallocate stack frame: 208 bytes
	addi   sp, sp, 208
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
.section .rodata
str_0:
	.asciz "ok\n"
	.align 1
.section .text