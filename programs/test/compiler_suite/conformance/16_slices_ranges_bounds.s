.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 224 bytes
	addi   sp, sp, -224
; Save return address (ra) at offset 216
	sd     ra, 216(sp)
; Save callee-saved register s2 at offset 184
	sd     s2, 184(sp)
; Save callee-saved register s3 at offset 192
	sd     s3, 192(sp)
; Save callee-saved register s4 at offset 200
	sd     s4, 200(sp)
; Save callee-saved register s5 at offset 208
	sd     s5, 208(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: buf
	addi   t0, sp, 8
; Store i8 to memory
	addi   t1, zero, 1
	sb     t1, 0(t0)
	addi   t0, sp, 9
; Store i8 to memory
	addi   t1, zero, 2
	sb     t1, 0(t0)
	addi   t0, sp, 10
; Store i8 to memory
	addi   t1, zero, 3
	sb     t1, 0(t0)
	addi   t0, sp, 11
; Store i8 to memory
	addi   t1, zero, 4
	sb     t1, 0(t0)
	addi   t0, sp, 12
; Store i8 to memory
	addi   t1, zero, 5
	sb     t1, 0(t0)
	addi   t0, sp, 13
; Store i8 to memory
	addi   t1, zero, 6
	sb     t1, 0(t0)
	addi   t0, sp, 14
; Store i8 to memory
	addi   t1, zero, 7
	sb     t1, 0(t0)
	addi   t0, sp, 15
; Store i8 to memory
	addi   t1, zero, 8
	sb     t1, 0(t0)
	addi   t0, sp, 0
; Store i8[8] to memory
	ld     t1, 8(sp)
	sd     t1, 0(t0)
; local var: s
	addi   t0, zero, 8
	addi   t1, zero, 8
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
	addi   t1, zero, 8
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
	addi   t0, zero, 8
	addi   t1, zero, 0
	sub    s2, t0, t1
	addi   t0, sp, 0
	addi   t1, zero, 0
	addi   t2, t1, 0
	add    s3, t0, t2
	addi   t0, sp, 32
; Store i8* to memory
	sd     s3, 0(t0)
	addi   t0, sp, 40
; Store i64 to memory
	sd     s2, 0(t0)
	addi   t0, sp, 16
; Store i8[] to memory
	addi   t1, s3, 0
	sd     t1, 0(t0)
	addi   t2, s2, 0
	sd     t2, 8(t0)
; Access field 'len' at offset 8
; Load i64 from memory into $$6
	addi   t0, sp, 16
	addi   t1, t0, 8
	addi   s2, t2, 0
	addi   t0, zero, 8
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_4
	j main__if_merge_6
; Basic Block: if_then_4
main__if_then_4:
	addi   t2, zero, 1
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_6
main__if_merge_6:
; Load i8* from memory into $$8
	addi   t0, sp, 16
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$9
	addi   t0, sp, 16
	addi   t1, t0, 8
	ld     s3, 0(t1)
	addi   t0, zero, 0
	sltu   t1, t0, s3
	sltiu  s3, t1, 1
	bne s3, zero, main__trap_7
	j main__trap_ok_8
; Basic Block: trap_7
main__trap_7:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_8
main__trap_ok_8:
	addi   t0, zero, 0
	addi   t1, t0, 0
	add    s2, s2, t1
; Load i8 from memory into $$12
	lb     s2, 0(s2)
	addi   t0, zero, 1
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_9
	j main__if_merge_11
; Basic Block: if_then_9
main__if_then_9:
	addi   t2, zero, 2
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_11
main__if_merge_11:
; Load i8* from memory into $$14
	addi   t0, sp, 16
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$15
	addi   t0, sp, 16
	addi   t1, t0, 8
	ld     s3, 0(t1)
	addi   t0, zero, 7
	sltu   t1, t0, s3
	sltiu  s3, t1, 1
	bne s3, zero, main__trap_12
	j main__trap_ok_13
; Basic Block: trap_12
main__trap_12:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_13
main__trap_ok_13:
	addi   t0, zero, 7
	addi   t1, t0, 0
	add    s2, s2, t1
; Load i8 from memory into $$18
	lb     s2, 0(s2)
	addi   t0, zero, 8
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_14
	j main__if_merge_16
; Basic Block: if_then_14
main__if_then_14:
	addi   t2, zero, 3
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_16
main__if_merge_16:
; local var: head
; Load i8* from memory into $$20
	addi   t0, sp, 16
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$21
	addi   t0, sp, 16
	addi   t1, t0, 8
	ld     s3, 0(t1)
	addi   t0, zero, 5
	sltu   s3, s3, t0
	bne s3, zero, main__trap_17
	j main__trap_ok_18
; Basic Block: trap_17
main__trap_17:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_18
main__trap_ok_18:
	addi   t0, zero, 2
	addi   t1, zero, 5
	sltu   s3, t1, t0
	bne s3, zero, main__trap_19
	j main__trap_ok_20
; Basic Block: trap_19
main__trap_19:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_20
main__trap_ok_20:
; sub operation on i64
	addi   t0, zero, 5
	addi   t1, zero, 2
	sub    s3, t0, t1
	addi   t0, zero, 2
	addi   t1, t0, 0
	add    s2, s2, t1
	addi   t0, sp, 64
; Store i8* to memory
	sd     s2, 0(t0)
	addi   t0, sp, 72
; Store i64 to memory
	sd     s3, 0(t0)
	addi   t0, sp, 48
; Store i8[] to memory
	addi   t1, s2, 0
	sd     t1, 0(t0)
	addi   t2, s3, 0
	sd     t2, 8(t0)
; Access field 'len' at offset 8
; Load i64 from memory into $$27
	addi   t0, sp, 48
	addi   t1, t0, 8
	addi   s2, t2, 0
	addi   t0, zero, 3
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_21
	j main__if_merge_23
; Basic Block: if_then_21
main__if_then_21:
	addi   t2, zero, 4
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_23
main__if_merge_23:
; Load i8* from memory into $$29
	addi   t0, sp, 48
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$30
	addi   t0, sp, 48
	addi   t1, t0, 8
	ld     s3, 0(t1)
	addi   t0, zero, 0
	sltu   t1, t0, s3
	sltiu  s3, t1, 1
	bne s3, zero, main__trap_24
	j main__trap_ok_25
; Basic Block: trap_24
main__trap_24:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_25
main__trap_ok_25:
	addi   t0, zero, 0
	addi   t1, t0, 0
	add    s2, s2, t1
; Load i8 from memory into $$33
	lb     s2, 0(s2)
	addi   t0, zero, 3
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_26
	j main__if_merge_28
; Basic Block: if_then_26
main__if_then_26:
	addi   t2, zero, 5
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_28
main__if_merge_28:
; local var: total
	addi   t0, sp, 80
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
; inferred local var: i
	addi   t0, sp, 88
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
; Access field 'len' at offset 8
; Load i64 from memory into $$35
	addi   t0, sp, 16
	addi   t1, t0, 8
	ld     s2, 0(t1)
; inferred local var: __for_end_0
	addi   t0, sp, 96
; Store i64 to memory
	sd     s2, 0(t0)
	j main__while_cond_29
; Basic Block: while_cond_29
main__while_cond_29:
; Load i32 from memory into $$36
	addi   t0, sp, 88
	lw     s2, 0(t0)
; Load i64 from memory into $$37
	addi   t0, sp, 96
	ld     s3, 0(t0)
	slt    s2, s2, s3
	bne s2, zero, main__while_body_30
	j main__while_exit_31
; Basic Block: while_body_30
main__while_body_30:
; Load i32 from memory into $$39
	addi   t0, sp, 80
	lw     s3, 0(t0)
; Load i32 from memory into $$40
	addi   t0, sp, 88
	lw     s2, 0(t0)
; Load i8* from memory into $$41
	addi   t0, sp, 16
	addi   t1, t0, 0
	ld     s4, 0(t1)
; Load i64 from memory into $$42
	addi   t0, sp, 16
	addi   t1, t0, 8
	ld     s5, 0(t1)
	sltu   t0, s2, s5
	sltiu  s5, t0, 1
	bne s5, zero, main__trap_32
	j main__trap_ok_33
; Basic Block: trap_32
main__trap_32:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_33
main__trap_ok_33:
	addi   t0, s2, 0
	add    s2, s4, t0
; Load i8 from memory into $$45
	lb     s2, 0(s2)
	addiw  s2, s2, 0
	addiw  s2, s2, 0
; add operation on i32
	add    s2, s3, s2
	addiw  s2, s2, 0
	addi   t0, sp, 80
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$48
	addi   t0, sp, 88
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 88
; Store i32 to memory
	sw     s2, 0(t0)
	j main__while_cond_29
; Basic Block: while_exit_31
main__while_exit_31:
; Load i32 from memory into $$50
	addi   t0, sp, 80
	lw     s2, 0(t0)
	addi   t0, zero, 36
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_34
	j main__if_merge_36
; Basic Block: if_then_34
main__if_then_34:
	addi   t2, zero, 6
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_36
main__if_merge_36:
; local var: sub
; Load i8* from memory into $$52
	addi   t0, sp, 16
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$53
	addi   t0, sp, 16
	addi   t1, t0, 8
	ld     s3, 0(t1)
	addi   t0, zero, 4
	sltu   s3, s3, t0
	bne s3, zero, main__trap_37
	j main__trap_ok_38
; Basic Block: trap_37
main__trap_37:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_38
main__trap_ok_38:
	addi   t0, zero, 1
	addi   t1, zero, 4
	sltu   s3, t1, t0
	bne s3, zero, main__trap_39
	j main__trap_ok_40
; Basic Block: trap_39
main__trap_39:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_40
main__trap_ok_40:
; sub operation on i64
	addi   t0, zero, 4
	addi   t1, zero, 1
	sub    s3, t0, t1
	addi   t0, zero, 1
	addi   t1, t0, 0
	add    s2, s2, t1
	addi   t0, sp, 120
; Store i8* to memory
	sd     s2, 0(t0)
	addi   t0, sp, 128
; Store i64 to memory
	sd     s3, 0(t0)
	addi   t0, sp, 104
; Store i8[] to memory
	addi   t1, s2, 0
	sd     t1, 0(t0)
	addi   t2, s3, 0
	sd     t2, 8(t0)
; Load i8* from memory into $$59
	addi   t0, sp, 104
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$60
	addi   t0, sp, 104
	addi   t1, t0, 8
	addi   s3, t2, 0
	addi   t0, zero, 0
	sltu   t1, t0, s3
	sltiu  s3, t1, 1
	bne s3, zero, main__trap_41
	j main__trap_ok_42
; Basic Block: trap_41
main__trap_41:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_42
main__trap_ok_42:
	addi   t0, zero, 0
	addi   t1, t0, 0
	add    s2, s2, t1
; Load i8 from memory into $$63
	lb     s2, 0(s2)
	addi   t0, zero, 2
	sub    t1, s2, t0
	sltu   s2, zero, t1
; Load i8* from memory into $$65
	addi   t0, sp, 104
	addi   t1, t0, 0
	ld     s3, 0(t1)
; Load i64 from memory into $$66
	addi   t0, sp, 104
	addi   t1, t0, 8
	ld     s4, 0(t1)
	addi   t0, zero, 2
	sltu   t1, t0, s4
	sltiu  s4, t1, 1
	bne s4, zero, main__trap_43
	j main__trap_ok_44
; Basic Block: trap_43
main__trap_43:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_44
main__trap_ok_44:
	addi   t0, zero, 2
	addi   t1, t0, 0
	add    s3, s3, t1
; Load i8 from memory into $$69
	lb     s3, 0(s3)
	addi   t0, zero, 4
	sub    t1, s3, t0
	sltu   s3, zero, t1
; or operation on i1
	or     s2, s2, s3
	slli  s2, s2, 56
	srai  s2, s2, 56
	bne s2, zero, main__if_then_45
	j main__if_merge_47
; Basic Block: if_then_45
main__if_then_45:
	addi   t0, zero, 7
	addi   a0, t0, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_47
main__if_merge_47:
; local var: tail
; Load i8* from memory into $$72
	addi   t0, sp, 16
	addi   t1, t0, 0
	ld     s3, 0(t1)
; Load i64 from memory into $$73
	addi   t0, sp, 16
	addi   t1, t0, 8
	ld     s4, 0(t1)
; add operation on i64
	addi   t0, zero, 7
	addi   t1, zero, 1
	add    s2, t0, t1
	sltu   s4, s4, s2
	bne s4, zero, main__trap_48
	j main__trap_ok_49
; Basic Block: trap_48
main__trap_48:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_49
main__trap_ok_49:
	addi   t0, zero, 6
	sltu   s4, s2, t0
	bne s4, zero, main__trap_50
	j main__trap_ok_51
; Basic Block: trap_50
main__trap_50:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_51
main__trap_ok_51:
; sub operation on i64
	addi   t0, zero, 6
	sub    s2, s2, t0
	addi   t0, zero, 6
	addi   t1, t0, 0
	add    s3, s3, t1
	addi   t0, sp, 152
; Store i8* to memory
	sd     s3, 0(t0)
	addi   t0, sp, 160
; Store i64 to memory
	sd     s2, 0(t0)
	addi   t0, sp, 136
; Store i8[] to memory
	addi   t1, s3, 0
	sd     t1, 0(t0)
	addi   t2, s2, 0
	sd     t2, 8(t0)
; Access field 'len' at offset 8
; Load i64 from memory into $$80
	addi   t0, sp, 136
	addi   t1, t0, 8
	addi   s2, t2, 0
	addi   t0, zero, 2
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_52
	j main__if_merge_54
; Basic Block: if_then_52
main__if_then_52:
	addi   t2, zero, 8
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_54
main__if_merge_54:
; Load i8* from memory into $$82
	addi   t0, sp, 136
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$83
	addi   t0, sp, 136
	addi   t1, t0, 8
	ld     s3, 0(t1)
	addi   t0, zero, 1
	sltu   t1, t0, s3
	sltiu  s3, t1, 1
	bne s3, zero, main__trap_55
	j main__trap_ok_56
; Basic Block: trap_55
main__trap_55:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_56
main__trap_ok_56:
	addi   t0, zero, 1
	addi   t1, t0, 0
	add    s2, s2, t1
; Load i8 from memory into $$86
	lb     s2, 0(s2)
	addi   t0, zero, 8
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_57
	j main__if_merge_59
; Basic Block: if_then_57
main__if_then_57:
	addi   t2, zero, 9
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_59
main__if_merge_59:
; local var: idx
	addi   t0, sp, 168
; Store i64 to memory
	addi   t1, zero, 99
	sd     t1, 0(t0)
; local var: oob
; Load i64 from memory into $$88
	addi   t0, sp, 168
	addi   s2, t1, 0
; Load i8* from memory into $$89
	addi   t0, sp, 16
	addi   t1, t0, 0
	ld     s3, 0(t1)
; Load i64 from memory into $$90
	addi   t0, sp, 16
	addi   t1, t0, 8
	ld     s4, 0(t1)
	sltu   t0, s2, s4
	sltiu  s4, t0, 1
	bne s4, zero, main__trap_60
	j main__trap_ok_61
; Basic Block: trap_60
main__trap_60:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_61
main__trap_ok_61:
	addi   t0, s2, 0
	add    s2, s3, t0
; Load i8 from memory into $$93
	lb     s2, 0(s2)
	addi   t0, sp, 176
; Store i8 to memory
	sb     s2, 0(t0)
; Load i8 from memory into $$94
	addi   t0, sp, 176
	lb     s2, 0(t0)
	slli  s2, s2, 56
	srli  s2, s2, 56
	addiw  s2, s2, 0
	addi   a0, s2, 0
; --- Function Epilogue ---
; Restore callee-saved register s5 from offset 208
	ld     s5, 208(sp)
; Restore callee-saved register s4 from offset 200
	ld     s4, 200(sp)
; Restore callee-saved register s3 from offset 192
	ld     s3, 192(sp)
; Restore callee-saved register s2 from offset 184
	ld     s2, 184(sp)
; Restore return address (ra) from offset 216
	ld     ra, 216(sp)
; Deallocate stack frame: 224 bytes
	addi   sp, sp, 224
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function