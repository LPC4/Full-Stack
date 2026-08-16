.section .text
; Function: halve
halve:
; --- Function Prologue ---
; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
; Save return address (ra) at offset 48
	sd     ra, 48(sp)
; Save callee-saved register s2 at offset 40
	sd     s2, 40(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t0, sp, 64
; Move parameter '$n' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
halve__entry:
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
; mod operation on i32
	addi   t0, zero, 2
	rem    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, zero, 0
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, halve__if_then_0
	j halve__if_merge_2
; Basic Block: if_then_0
halve__if_then_0:
	addi   t0, sp, 8
; Store i64 to memory
	addi   t1, zero, 1
	sd     t1, 0(t0)
; Load i32 from memory into $$4
	addi   t0, sp, 0
	lw     s2, 0(t0)
	addi   t0, sp, 16
; Store i32 to memory
	sw     s2, 0(t0)
	addi   a0, t1, 0
	ld     a1, 16(sp)
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 40
	ld     s2, 40(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_2
halve__if_merge_2:
	addi   t0, sp, 24
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
; Load i32 from memory into $$6
	addi   t0, sp, 0
	lw     s2, 0(t0)
; sdiv operation on i32
	addi   t0, zero, 2
	div    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 32
; Store i32 to memory
	sw     s2, 0(t0)
	addi   a0, t1, 0
	ld     a1, 32(sp)
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 40
	ld     s2, 40(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: quarter
quarter:
; --- Function Prologue ---
; Allocate stack frame: 96 bytes
	addi   sp, sp, -96
; Save return address (ra) at offset 88
	sd     ra, 88(sp)
; Save callee-saved register s2 at offset 80
	sd     s2, 80(sp)
; --- End Prologue ---
; --- Function Parameter Spills ---
	addi   t1, sp, 96
; Move parameter '$n' from register a0 to allocated register
	addiw  s2, a0, 0
; --- End Parameter Spills ---
; Basic Block: entry
quarter__entry:
	addi   t0, sp, 0
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$0
	addi   t0, sp, 0
	lw     s2, 0(t0)
; --- Function Call: halve ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, halve
; Unpacking small aggregate return from a0/a1
	sd     a0, 48(sp)
	sd     a1, 56(sp)
; --- End Function Call: halve ---
	addi   t0, sp, 8
; Store Result__i32__i32 to memory
	addi   t1, a0, 0
	sd     t1, 0(t0)
	addi   t2, a1, 0
	sd     t2, 8(t0)
; Load i64 from memory into $$3
	addi   t0, sp, 8
	addi   t1, t0, 0
	ld     s2, 0(t1)
	addi   t0, zero, 0
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, quarter__try_success_3
	j quarter__try_failure_4
; Basic Block: try_failure_4
quarter__try_failure_4:
	addi   t0, sp, 24
; Store i64 to memory
	addi   t1, zero, 1
	sd     t1, 0(t0)
; Load i32 from memory into $$6
	addi   t0, sp, 8
	addi   t1, t0, 8
	lw     s2, 0(t1)
	addi   t0, sp, 32
; Store i32 to memory
	sw     s2, 0(t0)
	ld     a0, 24(sp)
	ld     a1, 32(sp)
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 80
	ld     s2, 80(sp)
; Restore return address (ra) from offset 88
	ld     ra, 88(sp)
; Deallocate stack frame: 96 bytes
	addi   sp, sp, 96
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: try_success_3
quarter__try_success_3:
; Load i32 from memory into $$7
	addi   t0, sp, 8
	addi   t1, t0, 8
	lw     s2, 0(t1)
; inferred local var: h
	addi   t0, sp, 40
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$8
	addi   t0, sp, 40
	lw     s2, 0(t0)
; --- Function Call: halve ---
; Passing 1 arguments
	addi   a0, s2, 0
	jal ra, halve
; Unpacking small aggregate return from a0/a1
	sd     a0, 64(sp)
	sd     a1, 72(sp)
; --- End Function Call: halve ---
; --- Function Epilogue ---
; Restore callee-saved register s2 from offset 80
	ld     s2, 80(sp)
; Restore return address (ra) from offset 88
	ld     ra, 88(sp)
; Deallocate stack frame: 96 bytes
	addi   sp, sp, 96
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 272 bytes
	addi   sp, sp, -272
; Save return address (ra) at offset 256
	sd     ra, 256(sp)
; Save callee-saved register s2 at offset 240
	sd     s2, 240(sp)
; Save callee-saved register s3 at offset 248
	sd     s3, 248(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: t
	addi   t0, sp, 16
; Store i64 to memory
	addi   t1, zero, 1
	sd     t1, 0(t0)
	addi   t0, sp, 24
; Store i32 to memory
	addi   t1, zero, 15
	sw     t1, 0(t0)
	addi   t0, sp, 28
; Store i32 to memory
	addi   t1, zero, 27
	sw     t1, 0(t0)
	addi   t0, sp, 0
; Store Tree__i32 to memory
	ld     t1, 16(sp)
	sd     t1, 0(t0)
	ld     t2, 24(sp)
	sd     t2, 8(t0)
; Load i64 from memory into $$1
	addi   t0, sp, 0
	addi   t1, t0, 0
	ld     s2, 0(t1)
	addi   t0, zero, 0
	sub    t1, s2, t0
	sltiu  s3, t1, 1
	bne s3, zero, main__match_arm_6
	j main__match_next_7
; Basic Block: match_arm_6
main__match_arm_6:
; Load i32 from memory into $$3
	addi   t0, sp, 0
	addi   t1, t0, 8
	lw     s2, 0(t1)
	addi   t0, sp, 48
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$4
	addi   t0, sp, 48
	lw     s2, 0(t0)
	addi   t0, sp, 32
; Store i32 to memory
	sw     s2, 0(t0)
	j main__match_merge_5
; Basic Block: match_next_7
main__match_next_7:
	addi   t0, zero, 1
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__match_arm_8
	j main__match_next_9
; Basic Block: match_arm_8
main__match_arm_8:
; Load i32 from memory into $$6
	addi   t0, sp, 0
	addi   t1, t0, 8
	lw     s2, 0(t1)
	addi   t0, sp, 48
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$7
	addi   t0, sp, 0
	addi   t1, t0, 12
	lw     s2, 0(t1)
	addi   t0, sp, 56
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$8
	addi   t0, sp, 48
	lw     s2, 0(t0)
; Load i32 from memory into $$9
	addi   t0, sp, 56
	lw     s3, 0(t0)
; add operation on i32
	add    s2, s2, s3
	addiw  s2, s2, 0
	addi   t0, sp, 32
; Store i32 to memory
	sw     s2, 0(t0)
	j main__match_merge_5
; Basic Block: match_next_9
main__match_next_9:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: match_merge_5
main__match_merge_5:
; Load i32 from memory into $$11
	addi   t0, sp, 32
	lw     s2, 0(t0)
; inferred local var: sum
	addi   t0, sp, 64
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$12
	addi   t0, sp, 64
	lw     s2, 0(t0)
	addi   t0, zero, 42
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_10
	j main__if_merge_12
; Basic Block: if_then_10
main__if_then_10:
	addi   t2, zero, 1
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 248
	ld     s3, 248(sp)
; Restore callee-saved register s2 from offset 240
	ld     s2, 240(sp)
; Restore return address (ra) from offset 256
	ld     ra, 256(sp)
; Deallocate stack frame: 272 bytes
	addi   sp, sp, 272
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_12
main__if_merge_12:
; local var: opt
	addi   t0, sp, 88
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
	addi   t0, sp, 96
; Store i32 to memory
	addi   t1, zero, 41
	sw     t1, 0(t0)
	addi   t0, sp, 72
; Store Option__i32 to memory
	ld     t1, 88(sp)
	sd     t1, 0(t0)
	ld     t2, 96(sp)
	sd     t2, 8(t0)
; Load i64 from memory into $$15
	addi   t0, sp, 72
	addi   t1, t0, 0
	ld     s2, 0(t1)
	addi   t0, zero, 0
	sub    t1, s2, t0
	sltiu  s3, t1, 1
	bne s3, zero, main__match_arm_14
	j main__match_next_15
; Basic Block: match_arm_14
main__match_arm_14:
; Load i32 from memory into $$17
	addi   t0, sp, 72
	addi   t1, t0, 8
	lw     s2, 0(t1)
	addi   t0, sp, 112
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$18
	addi   t0, sp, 112
	lw     s2, 0(t0)
; add operation on i32
	addi   t0, zero, 1
	add    s2, s2, t0
	addiw  s2, s2, 0
	addi   t0, sp, 104
; Store i32 to memory
	sw     s2, 0(t0)
	j main__match_merge_13
; Basic Block: match_next_15
main__match_next_15:
	addi   t0, zero, 1
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__match_arm_16
	j main__match_next_17
; Basic Block: match_arm_16
main__match_arm_16:
	addi   t0, sp, 104
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	j main__match_merge_13
; Basic Block: match_next_17
main__match_next_17:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: match_merge_13
main__match_merge_13:
; Load i32 from memory into $$21
	addi   t0, sp, 104
	lw     s2, 0(t0)
; inferred local var: got
	addi   t0, sp, 120
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$22
	addi   t0, sp, 120
	lw     s2, 0(t0)
	addi   t0, zero, 42
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_18
	j main__if_merge_20
; Basic Block: if_then_18
main__if_then_18:
	addi   t2, zero, 2
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 248
	ld     s3, 248(sp)
; Restore callee-saved register s2 from offset 240
	ld     s2, 240(sp)
; Restore return address (ra) from offset 256
	ld     ra, 256(sp)
; Deallocate stack frame: 272 bytes
	addi   sp, sp, 272
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_20
main__if_merge_20:
; --- Function Call: quarter ---
; Passing 1 arguments
	addi   t0, zero, 100
	addi   a0, t0, 0
	jal ra, quarter
; Unpacking small aggregate return from a0/a1
	sd     a0, 208(sp)
	sd     a1, 216(sp)
; --- End Function Call: quarter ---
	addi   t0, sp, 136
; Store Result__i32__i32 to memory
	addi   t1, a0, 0
	sd     t1, 0(t0)
	addi   t2, a1, 0
	sd     t2, 8(t0)
; Load i64 from memory into $$26
	addi   t0, sp, 136
	addi   t1, t0, 0
	ld     s2, 0(t1)
	addi   t0, zero, 0
	sub    t1, s2, t0
	sltiu  s3, t1, 1
	bne s3, zero, main__match_arm_22
	j main__match_next_23
; Basic Block: match_arm_22
main__match_arm_22:
; Load i32 from memory into $$28
	addi   t0, sp, 136
	addi   t1, t0, 8
	lw     s2, 0(t1)
	addi   t0, sp, 152
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$29
	addi   t0, sp, 152
	lw     s2, 0(t0)
	addi   t0, sp, 128
; Store i32 to memory
	sw     s2, 0(t0)
	j main__match_merge_21
; Basic Block: match_next_23
main__match_next_23:
	addi   t0, zero, 1
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__match_arm_24
	j main__match_next_25
; Basic Block: match_arm_24
main__match_arm_24:
	addi   t0, sp, 128
; Store i32 to memory
	addi   t1, zero, 0
	sw     t1, 0(t0)
	j main__match_merge_21
; Basic Block: match_next_25
main__match_next_25:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: match_merge_21
main__match_merge_21:
; Load i32 from memory into $$31
	addi   t0, sp, 128
	lw     s2, 0(t0)
; inferred local var: ok
	addi   t0, sp, 160
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$32
	addi   t0, sp, 160
	lw     s2, 0(t0)
	addi   t0, zero, 25
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_26
	j main__if_merge_28
; Basic Block: if_then_26
main__if_then_26:
	addi   t2, zero, 3
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 248
	ld     s3, 248(sp)
; Restore callee-saved register s2 from offset 240
	ld     s2, 240(sp)
; Restore return address (ra) from offset 256
	ld     ra, 256(sp)
; Deallocate stack frame: 272 bytes
	addi   sp, sp, 272
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_28
main__if_merge_28:
; --- Function Call: quarter ---
; Passing 1 arguments
	addi   t0, zero, 10
	addi   a0, t0, 0
	jal ra, quarter
; Unpacking small aggregate return from a0/a1
	sd     a0, 224(sp)
	sd     a1, 232(sp)
; --- End Function Call: quarter ---
	addi   t0, sp, 176
; Store Result__i32__i32 to memory
	addi   t1, a0, 0
	sd     t1, 0(t0)
	addi   t2, a1, 0
	sd     t2, 8(t0)
; Load i64 from memory into $$36
	addi   t0, sp, 176
	addi   t1, t0, 0
	ld     s2, 0(t1)
	addi   t0, zero, 0
	sub    t1, s2, t0
	sltiu  s3, t1, 1
	bne s3, zero, main__match_arm_30
	j main__match_next_31
; Basic Block: match_arm_30
main__match_arm_30:
; Load i32 from memory into $$38
	addi   t0, sp, 176
	addi   t1, t0, 8
	lw     s2, 0(t1)
	addi   t0, sp, 192
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$39
	addi   t0, sp, 192
	lw     s2, 0(t0)
	addi   t0, sp, 168
; Store i32 to memory
	sw     s2, 0(t0)
	j main__match_merge_29
; Basic Block: match_next_31
main__match_next_31:
	addi   t0, zero, 1
	sub    t1, s2, t0
	sltiu  s2, t1, 1
	bne s2, zero, main__match_arm_32
	j main__match_next_33
; Basic Block: match_arm_32
main__match_arm_32:
	addi   t0, sp, 168
; Store i32 to memory
	addi   t1, zero, 7
	sw     t1, 0(t0)
	j main__match_merge_29
; Basic Block: match_next_33
main__match_next_33:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: match_merge_29
main__match_merge_29:
; Load i32 from memory into $$41
	addi   t0, sp, 168
	lw     s2, 0(t0)
; inferred local var: err
	addi   t0, sp, 200
; Store i32 to memory
	sw     s2, 0(t0)
; Load i32 from memory into $$42
	addi   t0, sp, 200
	lw     s2, 0(t0)
	addi   t0, zero, 7
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_34
	j main__if_merge_36
; Basic Block: if_then_34
main__if_then_34:
	addi   t2, zero, 4
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 248
	ld     s3, 248(sp)
; Restore callee-saved register s2 from offset 240
	ld     s2, 240(sp)
; Restore return address (ra) from offset 256
	ld     ra, 256(sp)
; Deallocate stack frame: 272 bytes
	addi   sp, sp, 272
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_36
main__if_merge_36:
	addi   t3, zero, 42
	addi   a0, t3, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 248
	ld     s3, 248(sp)
; Restore callee-saved register s2 from offset 240
	ld     s2, 240(sp)
; Restore return address (ra) from offset 256
	ld     ra, 256(sp)
; Deallocate stack frame: 272 bytes
	addi   sp, sp, 272
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function