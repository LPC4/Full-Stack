.section .text
; Function: main
main:
; --- Function Prologue ---
; Allocate stack frame: 112 bytes
	addi   sp, sp, -112
; Save return address (ra) at offset 96
	sd     ra, 96(sp)
; --- End Prologue ---
; Basic Block: entry
main__entry:
; local var: str1
	addi   t0, sp, 16
; Store i8* to memory
	la t1, str_0
	sd     t1, 0(t0)
	addi   t0, sp, 24
; Store i64 to memory
	addi   t1, zero, 11
	sd     t1, 0(t0)
	addi   t0, sp, 0
; Store i8[] to memory
	ld     t1, 16(sp)
	sd     t1, 0(t0)
	ld     t2, 24(sp)
	sd     t2, 8(t0)
; local var: str2
	addi   t0, sp, 48
; Store i8* to memory
	la t1, str_1
	sd     t1, 0(t0)
	addi   t0, sp, 56
; Store i64 to memory
	addi   t1, zero, 20
	sd     t1, 0(t0)
	addi   t0, sp, 32
; Store i8[] to memory
	ld     t1, 48(sp)
	sd     t1, 0(t0)
	ld     t2, 56(sp)
	sd     t2, 8(t0)
; local var: str3
	addi   t0, sp, 80
; Store i8* to memory
	la t1, str_2
	sd     t1, 0(t0)
	addi   t0, sp, 88
; Store i64 to memory
	addi   t1, zero, 0
	sd     t1, 0(t0)
	addi   t0, sp, 64
; Store i8[] to memory
	ld     t1, 80(sp)
	sd     t1, 0(t0)
	ld     t2, 88(sp)
	sd     t2, 8(t0)
	addi   t4, zero, 0
	addi   a0, t4, 0
; --- Function Epilogue ---
; Restore return address (ra) from offset 96
	ld     ra, 96(sp)
; Deallocate stack frame: 112 bytes
	addi   sp, sp, 112
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