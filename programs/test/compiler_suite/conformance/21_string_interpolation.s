.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 352 bytes
	addi   sp, sp, -352
; Save return address (ra) at offset 344
	sd     ra, 344(sp)
; Save callee-saved register s2 at offset 320
	sd     s2, 320(sp)
; Save callee-saved register s3 at offset 328
	sd     s3, 328(sp)
; Save callee-saved register s4 at offset 336
	sd     s4, 336(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: buffer
	addi   t0, sp, 32
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 33
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 34
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 35
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 36
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 37
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 38
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 39
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 40
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 41
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 42
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 43
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 44
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 45
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 46
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 47
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 48
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 49
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 50
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 51
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 52
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 53
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 54
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 55
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 56
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 57
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 58
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 59
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 60
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 61
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 62
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 63
; Store i8 to memory
	addi   t1, zero, 0
	sb     t1, 0(t0)
	addi   t0, sp, 0
; Store i8[32] to memory
	ld     t1, 32(sp)
	sd     t1, 0(t0)
	ld     t2, 40(sp)
	sd     t2, 8(t0)
	ld     t3, 48(sp)
	sd     t3, 16(t0)
	ld     t4, 56(sp)
	sd     t4, 24(t0)
; local var: value
	addi   t0, sp, 64
; Store i64 to memory
	addi   t1, zero, 42
	sd     t1, 0(t0)
	addi   t0, zero, 32
	addi   t1, zero, 32
	sltu   s2, t1, t0
	bne s2, zero, main__trap_0
	j main__trap_ok_1
; Basic Block: trap_0
main__trap_0:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_1
main__trap_ok_1:
	addi   t0, zero, 0
	addi   t1, zero, 32
	sltu   s2, t1, t0
	bne s2, zero, main__trap_2
	j main__trap_ok_3
; Basic Block: trap_2
main__trap_2:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_3
main__trap_ok_3:
; sub operation on i64
	addi   t0, zero, 32
	addi   t1, zero, 0
	sub    s2, t0, t1
	addi   t0, sp, 0
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    s3, t0, t2
	addi   t0, sp, 72
; Store i8* to memory
	sd     s3, 0(t0)
	addi   t0, sp, 80
; Store i64 to memory
	sd     s2, 0(t0)
	addi   t0, sp, 88
; Store i8[] to memory
	addi   t1, s3, 0
	sd     t1, 0(t0)
	addi   t2, s2, 0
	sd     t2, 8(t0)
; Load i64 from memory into $$8
	addi   t0, sp, 64
	ld     s3, 0(t0)
; --- Function Call: fmt_len_i64 ---
; Passing 1 arguments
	addi   a0, s3, 0
	jal ra, fmt_len_i64
	addi   s2, a0, 0
; --- End Function Call: fmt_len_i64 ---
; --- Function Call: fmt_len_add ---
; Passing 2 arguments
	addi   t0, zero, 6
	addi   a0, t0, 0
	addi   a1, s2, 0
	jal ra, fmt_len_add
	addi   s2, a0, 0
; --- End Function Call: fmt_len_add ---
; --- Function Call: fmt_is_len_overflow ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, fmt_is_len_overflow
	slli  s4, a0, 56
	srai  s4, s4, 56
; --- End Function Call: fmt_is_len_overflow ---
	bne s4, zero, main__format_overflow_5
	j main__format_size_ok_6
; Basic Block: format_overflow_5
main__format_overflow_5:
; --- Function Call: fmt_result_overflow ---
; Using sret convention for large aggregate return
	addi   t0, sp, 208
	addi   a0, t0, 0
; Passing 0 arguments
	jal ra, fmt_result_overflow
; --- End Function Call: fmt_result_overflow ---
	addi   t0, sp, 104
; Store Result__u64__FormatError to memory
	ld     t1, 208(sp)
	sd     t1, 0(t0)
	ld     t2, 216(sp)
	sd     t2, 8(t0)
	ld     t3, 224(sp)
	sd     t3, 16(t0)
	j main__format_done_4
; Basic Block: format_size_ok_6
main__format_size_ok_6:
; Load i64 from memory into $$13
	addi   t0, sp, 88
	addi   t1, t0, 8
	ld     s4, 0(t1)
	sltu   s4, s4, s2
	bne s4, zero, main__format_too_small_7
	j main__format_write_8
; Basic Block: format_too_small_7
main__format_too_small_7:
; --- Function Call: fmt_result_too_small ---
; Using sret convention for large aggregate return
	addi   t0, sp, 232
	addi   a0, t0, 0
; Passing 1 arguments
	addi   a1, s2, 0
	jal ra, fmt_result_too_small
; --- End Function Call: fmt_result_too_small ---
	addi   t0, sp, 104
; Store Result__u64__FormatError to memory
	ld     t1, 232(sp)
	sd     t1, 0(t0)
	ld     t2, 240(sp)
	sd     t2, 8(t0)
	ld     t3, 248(sp)
	sd     t3, 16(t0)
	j main__format_done_4
; Basic Block: format_write_8
main__format_write_8:
	addi   t0, sp, 128
; Store i8* to memory
	la t1, str_0
	sd     t1, 0(t0)
	addi   t0, sp, 136
; Store i64 to memory
	addi   t1, zero, 6
	sd     t1, 0(t0)
; --- Function Call: fmt_write_bytes ---
; Passing 3 arguments
	addi   t0, sp, 72
	addi   a0, t0, 0
	addi   t1, zero, 0
	addi   a1, t1, 0
	addi   t2, sp, 128
	addi   a2, t2, 0
	jal ra, fmt_write_bytes
	addi   s4, a0, 0
; --- End Function Call: fmt_write_bytes ---
; --- Function Call: fmt_write_i64 ---
; Passing 3 arguments
	addi   t0, sp, 72
	addi   a0, t0, 0
	addi   a1, s4, 0
	addi   a2, s3, 0
	jal ra, fmt_write_i64
	addi   s3, a0, 0
; --- End Function Call: fmt_write_i64 ---
; --- Function Call: fmt_result_ok ---
; Using sret convention for large aggregate return
	addi   t0, sp, 256
	addi   a0, t0, 0
; Passing 1 arguments
	addi   a1, s2, 0
	jal ra, fmt_result_ok
; --- End Function Call: fmt_result_ok ---
	addi   t0, sp, 104
; Store Result__u64__FormatError to memory
	ld     t1, 256(sp)
	sd     t1, 0(t0)
	ld     t2, 264(sp)
	sd     t2, 8(t0)
	ld     t3, 272(sp)
	sd     t3, 16(t0)
	j main__format_done_4
; Basic Block: format_done_4
main__format_done_4:
; Load { tag: i64, payload: i8[16] } from memory into $$20
	addi   t0, sp, 104
	ld     t1, 0(t0)
	sd     t1, 280(sp)
	ld     t2, 8(t0)
	sd     t2, 288(sp)
	ld     t3, 16(t0)
	sd     t3, 296(sp)
; inferred local var: written
	addi   t0, sp, 144
; Store Result__u64__FormatError to memory
	sd     t1, 0(t0)
	sd     t2, 8(t0)
	sd     t3, 16(t0)
; Load i64 from memory into $$21
	addi   t0, sp, 144
	addi   t1, t0, 0
	ld     s2, 0(t1)
	addi   t0, zero, 0
	sub    t1, s2, t0
	sltiu  s3, t1, 1
	bne s3, zero, main__match_arm_10
	j main__match_next_11
; Basic Block: match_arm_10
main__match_arm_10:
; Load i64 from memory into $$23
	addi   t0, sp, 144
	addi   t1, t0, 8
	ld     s2, 0(t1)
	addi   t0, sp, 168
; Store i64 to memory
	sd     s2, 0(t0)
	addi   t0, zero, 32
	addi   t1, zero, 32
	sltu   s2, t1, t0
	bne s2, zero, main__trap_12
	j main__trap_ok_13
; Basic Block: trap_12
main__trap_12:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_13
main__trap_ok_13:
	addi   t0, zero, 0
	addi   t1, zero, 32
	sltu   s2, t1, t0
	bne s2, zero, main__trap_14
	j main__trap_ok_15
; Basic Block: trap_14
main__trap_14:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_15
main__trap_ok_15:
; sub operation on i64
	addi   t0, zero, 32
	addi   t1, zero, 0
	sub    s2, t0, t1
	addi   t0, sp, 0
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    s3, t0, t2
	addi   t0, sp, 176
; Store i8* to memory
	sd     s3, 0(t0)
	addi   t0, sp, 184
; Store i64 to memory
	sd     s2, 0(t0)
; Load i64 from memory into $$29
	addi   t0, sp, 168
	ld     s2, 0(t0)
; --- Function Call: fmt_subslice ---
; Passing 3 arguments
	addi   t0, sp, 176
	addi   a0, t0, 0
	addi   t1, zero, 0
	addi   a1, t1, 0
	addi   a2, s2, 0
	jal ra, fmt_subslice
; Unpacking small aggregate return from a0/a1
	sd     a0, 304(sp)
	sd     a1, 312(sp)
; --- End Function Call: fmt_subslice ---
; --- Function Call: console_print ---
; Passing 1 arguments
	addi   t0, sp, 304
	addi   a0, t0, 0
	jal ra, console_print
	addiw  s2, a0, 0
; --- End Function Call: console_print ---
	addi   t0, sp, 192
; Store i8* to memory
	la t1, str_1
	sd     t1, 0(t0)
	addi   t0, sp, 200
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
; --- Function Call: console_println ---
; Passing 1 arguments
	addi   t0, sp, 192
	addi   a0, t0, 0
	jal ra, console_println
	addiw  s2, a0, 0
; --- End Function Call: console_println ---
	addi   t1, zero, 42
	addi   a0, t1, 0
; --- Function Epilogue ---
; Restore callee-saved register s4 from offset 336
	ld     s4, 336(sp)
; Restore callee-saved register s3 from offset 328
	ld     s3, 328(sp)
; Restore callee-saved register s2 from offset 320
	ld     s2, 320(sp)
; Restore return address (ra) from offset 344
	ld     ra, 344(sp)
; Deallocate stack frame: 352 bytes
	addi   sp, sp, 352
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: match_next_11
main__match_next_11:
	addi   t0, zero, 1
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__match_arm_16
	j main__match_next_17
; Basic Block: match_arm_16
main__match_arm_16:
	addi   t2, zero, 1
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s4 from offset 336
	ld     s4, 336(sp)
; Restore callee-saved register s3 from offset 328
	ld     s3, 328(sp)
; Restore callee-saved register s2 from offset 320
	ld     s2, 320(sp)
; Restore return address (ra) from offset 344
	ld     ra, 344(sp)
; Deallocate stack frame: 352 bytes
	addi   sp, sp, 352
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: match_next_17
main__match_next_17:
	li a0, 134
	li a7, 93
	ecall
; End of function
.section .rodata
str_0:
	.asciz "value="
	.align 1
str_1:
	.asciz ""
	.align 1
.section .text
