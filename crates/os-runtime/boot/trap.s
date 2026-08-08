# M-mode trap handler at ROM offset 0x100 (concatenated after startup.s).


# Handles ecall from U/S/M-mode; all other traps mret straight back.
# mtvec is loaded by _start (kernel) or Pipeline::new (hosted).
_m_trap:
	csrr t0, mcause
	li t1, 8
	beq t0, t1, _dispatch_ecall
	li t1, 9
	beq t0, t1, _dispatch_ecall
	li t1, 11
	beq t0, t1, _dispatch_ecall
	mret

_dispatch_ecall:
	li t0, 64
	beq a7, t0, sys_write
	li t0, 93
	beq a7, t0, sys_exit
	li t0, 94
	beq a7, t0, sys_exit
	li t0, 214
	beq a7, t0, sys_brk
	j sys_unknown

# sys_write(fd=a0, buf=a1, len=a2) -> bytes written
sys_write:
	li t0, 1
	bne a0, t0, _write_error
	li t0, 268435456
	mv t1, a1
	mv t2, a2
	mv t3, a2
_write_loop:
	beqz t2, _write_done
	lb t4, 0(t1)
	sb t4, 0(t0)
	addi t1, t1, 1
	addi t2, t2, -1
	j _write_loop
_write_done:
	mv a0, t3
	j _advance_mepc_and_mret
_write_error:
	li a0, -1
	j _advance_mepc_and_mret

# sys_exit(code=a0) - write to SYSCON, bus halts VM
sys_exit:
	li t0, 268500992
	sd a0, 0(t0)
	j _advance_mepc_and_mret

# sys_brk(addr=a0) -> break; addr==0 queries. See the OS specification, 7.8.
# The address is built as (1<<31) + 0x01FFFFF8 so li never sign-extends bit 31.
sys_brk:
	li t0, 1
	slli t0, t0, 31
	li t1, 33554424
	add t0, t0, t1
	beqz a0, _brk_query
	bgeu a0, t0, _brk_query
	sd a0, 0(t0)
	j _advance_mepc_and_mret
_brk_query:
	ld a0, 0(t0)
	j _advance_mepc_and_mret

sys_unknown:
	li a0, -1
	j _advance_mepc_and_mret

_advance_mepc_and_mret:
	csrr t0, mepc
	addi t0, t0, 4
	csrw mepc, t0
	mret
