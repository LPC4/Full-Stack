.section .text
main:
	addi   sp, sp, -16
	sd     ra, 8(sp)
	sd     s2, 0(sp)

main__entry:
	addi   t0, zero, 1
	addi   t1, zero, 1
	add    s2, t0, t1
	addiw  s2, s2, 0
	addi   a0, s2, 0

	ld     s2, 0(sp)
	ld     ra, 8(sp)
	addi   sp, sp, 16
	jalr   zero, 0(ra)
