.section .text
.globl main
main:
	addi   sp, sp, -32
	sd     ra, 16(sp)
	sd     s0, 24(sp)
	addi   s0, sp, 0
main__entry:
	; local var: boxed
	; local var: legacy
	; local var: double
	addi   t0, zero, 0
	addi   a0, t0, 0
	ld     s0, 24(sp)
	ld     ra, 16(sp)
	addi   sp, sp, 32
	jalr   zero, 0(ra)