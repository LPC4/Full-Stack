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
	addi   t2, sp, 24
	addi   t3, zero, 11
	sd     t3, 0(t2)
	addi   t4, sp, 0
	ld     t5, 16(sp)
	sd     t5, 0(t4)
	ld     t6, 24(sp)
	sd     t6, 8(t4)
	; local var: str2
	addi   t1, sp, 48
	la t2, str_1
	sd     t2, 0(t1)
	addi   t3, sp, 56
	addi   t4, zero, 20
	sd     t4, 0(t3)
	addi   t5, sp, 32
	ld     t6, 48(sp)
	sd     t6, 0(t5)
	ld     t0, 56(sp)
	sd     t0, 8(t5)
	; local var: str3
	addi   t2, sp, 80
	la t3, str_2
	sd     t3, 0(t2)
	addi   t4, sp, 88
	addi   t5, zero, 0
	sd     t5, 0(t4)
	addi   t6, sp, 64
	ld     t0, 80(sp)
	sd     t0, 0(t6)
	ld     t1, 88(sp)
	sd     t1, 8(t6)
	addi   t3, zero, 0
	addi   a0, t3, 0
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