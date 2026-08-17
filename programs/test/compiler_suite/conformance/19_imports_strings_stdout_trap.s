.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
; Save return address (ra) at offset 48
	sd     ra, 48(sp)
; Save callee-saved register s2 at offset 32
	sd     s2, 32(sp)
; Save callee-saved register s3 at offset 40
	sd     s3, 40(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: greeting
	addi   t0, sp, 16
; Store i8* to memory
	la t1, str_0
	sd     t1, 0(t0)
	addi   t0, sp, 24
; Store i64 to memory
	addi   t1, zero, 5
	sd     t1, 0(t0)
	addi   t0, sp, 0
; Store i8[] to memory
	ld     t1, 16(sp)
	sd     t1, 0(t0)
	ld     t2, 24(sp)
	sd     t2, 8(t0)
; Access field 'len' at offset 8
; Load i64 from memory into $$1
	addi   t0, sp, 0
	addi   t1, t0, 8
	addi   s2, t2, 0
	addi   t0, zero, 5
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_0
	j main__if_merge_2
; Basic Block: if_then_0
main__if_then_0:
	addi   t2, zero, 1
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 40
	ld     s3, 40(sp)
; Restore callee-saved register s2 from offset 32
	ld     s2, 32(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_2
main__if_merge_2:
; Load i8* from memory into $$3
	addi   t0, sp, 0
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$4
	addi   t0, sp, 0
	addi   t1, t0, 8
	ld     s3, 0(t1)
	addi   t0, zero, 0
	sltu   t1, t0, s3
	sltiu  s3, t1, 1
	bne s3, zero, main__trap_3
	j main__trap_ok_4
; Basic Block: trap_3
main__trap_3:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_4
main__trap_ok_4:
	addi   t0, zero, 0
	addi   t1, t0, 0
	add    s2, s2, t1
; Load i8 from memory into $$7
	lb     s2, 0(s2)
	addi   t0, zero, 104
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_5
	j main__if_merge_7
; Basic Block: if_then_5
main__if_then_5:
	addi   t2, zero, 2
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 40
	ld     s3, 40(sp)
; Restore callee-saved register s2 from offset 32
	ld     s2, 32(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_7
main__if_merge_7:
; Load i8* from memory into $$9
	addi   t0, sp, 0
	addi   t1, t0, 0
	ld     s2, 0(t1)
; Load i64 from memory into $$10
	addi   t0, sp, 0
	addi   t1, t0, 8
	ld     s3, 0(t1)
	addi   t0, zero, 4
	sltu   t1, t0, s3
	sltiu  s3, t1, 1
	bne s3, zero, main__trap_8
	j main__trap_ok_9
; Basic Block: trap_8
main__trap_8:
	li a0, 134
	li a7, 93
	ecall
; Basic Block: trap_ok_9
main__trap_ok_9:
	addi   t0, zero, 4
	addi   t1, t0, 0
	add    s2, s2, t1
; Load i8 from memory into $$13
	lb     s2, 0(s2)
	addi   t0, zero, 111
	sub    t1, s2, t0
	sltu   s2, zero, t1
	bne s2, zero, main__if_then_10
	j main__if_merge_12
; Basic Block: if_then_10
main__if_then_10:
	addi   t2, zero, 3
	addi   a0, t2, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 40
	ld     s3, 40(sp)
; Restore callee-saved register s2 from offset 32
	ld     s2, 32(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; Basic Block: if_merge_12
main__if_merge_12:
; --- Function Call: console_println ---
; Passing 1 arguments
	addi   t0, sp, 0
	addi   a0, t0, 0
	jal ra, console_println
	addiw  s2, a0, 0
; --- End Function Call: console_println ---
	addi   t0, zero, 0
	sltiu  s2, t0, 1
	slli  s2, s2, 56
	srai  s2, s2, 56
	bne s2, zero, main__trap_13
	j main__trap_ok_14
; Basic Block: trap_13
main__trap_13:
	li a0, 101
	li a7, 93
	ecall
; Basic Block: trap_ok_14
main__trap_ok_14:
	addi   t1, zero, 0
	addi   a0, t1, 0
; --- Function Epilogue ---
; Restore callee-saved register s3 from offset 40
	ld     s3, 40(sp)
; Restore callee-saved register s2 from offset 32
	ld     s2, 32(sp)
; Restore return address (ra) from offset 48
	ld     ra, 48(sp)
; Deallocate stack frame: 64 bytes
	addi   sp, sp, 64
; Return to caller
	jalr   zero, 0(ra)
; --- End Epilogue ---
; End of function
.section .rodata
str_0:
	.asciz "hello"
	.align 1
.section .text