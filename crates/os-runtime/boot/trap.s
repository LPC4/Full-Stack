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
    li t0, 1000
    beq a7, t0, sys_putchar
    li t0, 1001
    beq a7, t0, sys_puts
    li t0, 1002
    beq a7, t0, sys_printf
    j sys_unknown

sys_putchar:
    li t0, 268435456
    sb a0, 0(t0)
    li a0, 0
    j _advance_mepc_and_mret

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

# sys_puts(ptr=a0) - null-terminated string + newline
sys_puts:
    li t0, 268435456
    mv t1, a0
_puts_loop:
    lb t2, 0(t1)
    beqz t2, _puts_newline
    sb t2, 0(t0)
    addi t1, t1, 1
    j _puts_loop
_puts_newline:
    li t2, 10
    sb t2, 0(t0)
    li a0, 0
    j _advance_mepc_and_mret

# sys_exit(code=a0) - write to SYSCON, bus halts VM
sys_exit:
    li t0, 268500992
    sd a0, 0(t0)
    j _advance_mepc_and_mret

sys_unknown:
    li a0, -1
    j _advance_mepc_and_mret

# sys_printf(fmt=a0, a1..a6 = up to 6 args)
# a1..a6 are spilled to 0..40(sp) before the loop. t2 is the byte offset
# into that spill area (0, 8, 16, ...), advanced by 8 each time a format
# specifier consumes an argument. t0 = UART, t1 = fmt pointer.
sys_printf:
    addi sp, sp, -48
    sd a1, 0(sp)
    sd a2, 8(sp)
    sd a3, 16(sp)
    sd a4, 24(sp)
    sd a5, 32(sp)
    sd a6, 40(sp)
    li t0, 268435456    # UART base
    mv t1, a0           # fmt pointer
    li t2, 0            # arg byte offset
_printf_loop:
    lb t3, 0(t1)
    addi t1, t1, 1
    beqz t3, _printf_done
    li t4, 37
    bne t3, t4, _printf_emit_char
    lb t3, 0(t1)
    addi t1, t1, 1
    li t4, 37
    beq t3, t4, _printf_percent
    add t4, sp, t2
    ld t4, 0(t4)
    addi t2, t2, 8
    li t5, 100
    beq t3, t5, _printf_fmt_d
    li t5, 105
    beq t3, t5, _printf_fmt_d
    li t5, 117
    beq t3, t5, _printf_fmt_u
    li t5, 120
    beq t3, t5, _printf_fmt_x
    li t5, 88
    beq t3, t5, _printf_fmt_x
    li t5, 112
    beq t3, t5, _printf_fmt_p
    li t5, 99
    beq t3, t5, _printf_fmt_c
    li t5, 115
    beq t3, t5, _printf_fmt_s
    j _printf_fmt_unknown
_printf_emit_char:
    sb t3, 0(t0)
    j _printf_loop
_printf_percent:
    li t5, 37
    sb t5, 0(t0)
    j _printf_loop
_printf_fmt_c:
    sb t4, 0(t0)
    j _printf_loop
_printf_fmt_s:
    mv t5, t4
_printf_fmt_s_loop:
    lb t6, 0(t5)
    beqz t6, _printf_loop
    sb t6, 0(t0)
    addi t5, t5, 1
    j _printf_fmt_s_loop
_printf_fmt_unknown:
    li t5, 37
    sb t5, 0(t0)
    sb t3, 0(t0)
    j _printf_loop

# %d / %i : print '-' if negative, then fall through to %u
_printf_fmt_d:
    bge t4, x0, _printf_fmt_u
    li t5, 45           # '-'
    sb t5, 0(t0)
    sub t4, x0, t4      # negate
    j _printf_fmt_u

# %u : extract digits via divu/remu into a scratch buffer on the stack
#      (written low-to-high), then emit in reverse for correct order
_printf_fmt_u:
    bnez t4, _printf_uint_nonzero
    li t5, 48
    sb t5, 0(t0)
    j _printf_loop
_printf_uint_nonzero:
    addi sp, sp, -40
    sd t1, 24(sp)
    sd t2, 32(sp)
    mv t1, sp
    li t2, 0
    li t6, 10
_printf_uint_loop:
    beqz t4, _printf_uint_emit
    remu t5, t4, t6
    divu t4, t4, t6
    addi t5, t5, 48
    sb t5, 0(t1)
    addi t1, t1, 1
    addi t2, t2, 1
    j _printf_uint_loop
_printf_uint_emit:
    addi t1, t1, -1
_printf_uint_emit_loop:
    beqz t2, _printf_uint_done
    lb t5, 0(t1)
    sb t5, 0(t0)
    addi t1, t1, -1
    addi t2, t2, -1
    j _printf_uint_emit_loop
_printf_uint_done:
    ld t1, 24(sp)
    ld t2, 32(sp)
    addi sp, sp, 40
    j _printf_loop

# %x / %X : scan nibbles from bit 60 down to 0, skipping leading zeros
#           (t2 tracks whether any nonzero nibble has been seen yet)
_printf_fmt_x:
    addi sp, sp, -16
    sd t1, 8(sp)
    sd t2, 0(sp)
    li t1, 60
    li t2, 0
_printf_hex_loop:
    blt t1, x0, _printf_hex_post
    srl t6, t4, t1
    andi t6, t6, 15
    or t2, t2, t6
    beqz t2, _printf_hex_advance
    li t2, 1
    li t5, 10
    blt t6, t5, _printf_hex_digit
    addi t6, t6, 87
    j _printf_hex_emit
_printf_hex_digit:
    addi t6, t6, 48
_printf_hex_emit:
    sb t6, 0(t0)
_printf_hex_advance:
    addi t1, t1, -4
    j _printf_hex_loop
_printf_hex_post:
    bnez t2, _printf_hex_done
    li t5, 48
    sb t5, 0(t0)
_printf_hex_done:
    ld t1, 8(sp)
    ld t2, 0(sp)
    addi sp, sp, 16
    j _printf_loop

# %p : "0x" prefix then hex
_printf_fmt_p:
    li t5, 48
    sb t5, 0(t0)
    li t5, 120
    sb t5, 0(t0)
    j _printf_fmt_x

_printf_done:
    addi sp, sp, 48
    li a0, 0
    j _advance_mepc_and_mret

_advance_mepc_and_mret:
    csrr t0, mepc
    addi t0, t0, 4
    csrw mepc, t0
    mret
