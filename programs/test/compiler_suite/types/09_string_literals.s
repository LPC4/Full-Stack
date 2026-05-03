.section .text
	; ========================================
	; Function: main
	; ========================================
.globl main
main:
	; --- Function Prologue ---
	; Allocate stack frame: 64 bytes
	addi   sp, sp, -64
	; Save return address (ra) at offset 48
	sd     ra, 48(sp)
	; Save callee-saved register s8 at offset 56
	sd     s0, 56(sp)
	; Set up frame pointer
	addi   s0, sp, 0
	; --- End Prologue ---
	; --- Basic Block: entry ---
main__entry:
	; local var: str1
	addi   t0, sp, 8
	; Store i8* to memory
	la t1, str_0
	sd     t1, 0(t0)
	addi   t0, sp, 16
	; Store i64 to memory
	addi   t1, zero, 11
	sd     t1, 0(t0)
	addi   t0, sp, 0
	; Store {data: i8*, length: i64} to memory
	ld     t1, 8(sp)
	sd     t1, 0(t0)
	ld     t2, 16(sp)
	sd     t2, 8(t0)
	; local var: str2
	addi   t0, sp, 24
	; Store i8* to memory
	la t1, str_1
	sd     t1, 0(t0)
	addi   t0, sp, 32
	; Store i64 to memory
	addi   t1, zero, 20
	sd     t1, 0(t0)
	addi   t0, sp, 16
	; Store {data: i8*, length: i64} to memory
	ld     t1, 24(sp)
	sd     t1, 0(t0)
	ld     t2, 32(sp)
	sd     t2, 8(t0)
	; local var: str3
	addi   t0, sp, 40
	; Store i8* to memory
	la t1, str_2
	sd     t1, 0(t0)
	addi   t0, sp, 48
	; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
	addi   t0, sp, 32
	; Store {data: i8*, length: i64} to memory
	ld     t1, 40(sp)
	sd     t1, 0(t0)
	ld     t2, 48(sp)
	sd     t2, 8(t0)
	addi   t4, zero, 0
	addi   a0, t4, 0
	; --- Function Epilogue ---
	; Restore callee-saved register s8 from offset 56
	ld     s0, 56(sp)
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
	.asciz "Hello World"
	.align 1
str_1:
	.asciz "Line 1\nLine 2\tTabbed"
	.align 1
str_2:
	.asciz ""
	.align 1
.section .text