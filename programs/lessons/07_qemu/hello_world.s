# 06's hello world, ported to qemu-system-riscv64 -machine virt.
# Built with the GNU toolchain, not the IDE: see "The same code on QEMU" in guide/src/written.md.

.data
msg:
	.asciz "Hello, world!\n"

.text
.globl _start
_start:
	li   sp, 0x88000000  # virt gives 128 MB of RAM at 0x80000000, so this is the top
	la   a0, msg
	jal  ra, print_string
	li   a0, 0
	jal  ra, exit

print_char:            # print_char(c): unchanged from lesson 06
	li   t0, 0x10000000
	sb   a0, 0(t0)
	ret

print_string:          # print_string(s): unchanged from lesson 06
	addi sp, sp, -32
	sd   fp, 16(sp)
	addi fp, sp, 32
	sd   ra, -8(fp)
	sd   a0, -32(fp)
ps_loop:
	ld   t1, -32(fp)
	lb   a0, 0(t1)
	beqz a0, ps_done
	jal  ra, print_char
	ld   t1, -32(fp)
	addi t1, t1, 1
	sd   t1, -32(fp)
	j    ps_loop
ps_done:
	ld   ra, -8(fp)
	ld   fp, -16(fp)
	addi sp, sp, 32
	ret

exit:                  # exit(code): virt's poweroff device wants magic values, not raw codes
	li   t0, 0x100000    # the SiFive test finisher
	beqz a0, exit_pass
	slli a0, a0, 16      # a nonzero code rides in the upper 16 bits
	li   t1, 0x3333      # 0x3333 = "fail, code attached"
	or   a0, a0, t1      # a0 = [exit:16][magic:16] (can't use ori here, 0x3333 exceeds 12 bits)
	sw   a0, 0(t0)       # sw instead of sd
exit_pass:
	li   a0, 0x5555      # 0x5555 = "pass", qemu exits 0
	sw   a0, 0(t0)       # sw instead of sd
