# ROM firmware (M-mode) stage 2: trap handler and syscall dispatch.
#
# Concatenated after startup.s to form the complete ROM image.
# _m_trap lands at offset 0x100 due to the .space 192 pad in startup.s.

# _m_trap: M-mode trap handler at ROM offset 0x100.
# Handles ecall from U/S/M-mode (causes 8, 9, 11); all other traps -> mret.
# mtvec is loaded by _start (kernel) or directly by Pipeline::new (hosted).
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

sys_unknown:
    li a0, -1
    j _advance_mepc_and_mret

_advance_mepc_and_mret:
    csrr t0, mepc
    addi t0, t0, 4
    csrw mepc, t0
    mret
