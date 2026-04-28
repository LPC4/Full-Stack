.section .text
.globl main
main:
	addi   sp, sp, -112
	sd     ra, 96(sp)
	sd     s0, 104(sp)
	addi   s0, sp, 0
main__entry:
	; local var: str1
	addi   t0, sp, 16
	la t1, str_0
	sd     t1, 0(t0)
	addi   t0, sp, 24
	addi   t1, zero, 11
	sd     t1, 0(t0)
	addi   t0, sp, 0
	ld     t1, 16(sp)
	sd     t1, 0(t0)
	ld     t2, 24(sp)
	sd     t2, 8(t0)
	; local var: str2
	addi   t0, sp, 48
	la t1, str_1
	sd     t1, 0(t0)
	addi   t0, sp, 56
	addi   t1, zero, 20
	sd     t1, 0(t0)
	addi   t0, sp, 32
	ld     t1, 48(sp)
	sd     t1, 0(t0)
	ld     t2, 56(sp)
	sd     t2, 8(t0)
	; local var: str3
	addi   t0, sp, 80
	la t1, str_2
	sd     t1, 0(t0)
	addi   t0, sp, 88
	addi   t1, zero, 0
	sd     t1, 0(t0)
	addi   t0, sp, 64
	ld     t1, 80(sp)
	sd     t1, 0(t0)
	ld     t2, 88(sp)
	sd     t2, 8(t0)
	addi   t4, zero, 0
	addi   a0, t4, 0
	ld     s0, 104(sp)
	ld     ra, 96(sp)
	addi   sp, sp, 112
	jalr   zero, 0(ra)
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