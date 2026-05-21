# ROM firmware (M-mode).
#
# Memory map:
#   0x00000000  ROM — this file (256 KiB region)
#   0x02000000  CLINT
#   0x0C000000  PLIC
#   0x10000000  UART TX   (sb byte, 0(t0) emits one character)
#   0x10010000  SYSCON    (sd exit_code, 0(t0) halts the VM)
#   0x80000000  RAM       (ELF / kernel image)
#
# ROM layout:
#   0x000  _start    — kernel boot stub (runs when CPU starts at ROM_BASE)
#                      Sets PMP, delegates exceptions/interrupts, mrets to S-mode.
#                      Padded to exactly 0x100 bytes.
#   0x100  _m_trap   — M-mode trap handler (mtvec target for all non-kernel use
#                      and for SBI ecalls from the kernel in S-mode).
#
# Syscall ABI (Linux RV64 + custom extensions):
#   a7       = syscall number
#   a0-a6    = arguments (a0 also holds return value)
#   t0-t6    = scratch, clobbered by ROM handlers

    .section .text
    .globl _start
    .globl _m_trap

# ============================================================
# _start: M-mode kernel boot stub (offset 0x000)
#
# Runs when new_kernel() starts the CPU at ROM_BASE.
# Does exactly four things, then mrets into S-mode:
#   1. PMP  — grant S/U-mode RWX access to the full address space.
#   2. medeleg — delegate page faults + U-mode ecall to S-mode.
#   3. mideleg — delegate supervisor interrupts to S-mode.
#   4. mret into Supervisor mode at RAM_BASE (0x80000000).
#
# After mret the ROM is done until the kernel makes an SBI call
# (ecall from S-mode → cause=9 → _m_trap below).
# ============================================================
_start:
    # 1. PMP: single entry covering the full address space, RWX.
    #    pmpaddr0 = -1  (TOR upper bound = all ones = entire space)
    #    pmpcfg0  = 0x1F (A=11=NAPOT, X=1, W=1, R=1)
    li t0, -1
    csrw pmpaddr0, t0
    li t0, 31
    csrw pmpcfg0, t0

    # 2. medeleg: delegate to S-mode.
    #    bit  8 = ecall from U-mode   (user syscalls → kernel)
    #    bit 12 = instruction page fault
    #    bit 13 = load page fault
    #    bit 15 = store page fault
    #    decimal: 256 + 4096 + 8192 + 32768 = 45312
    li t0, 45312
    csrw medeleg, t0

    # 3. mideleg: delegate to S-mode.
    #    bit 1 = supervisor software interrupt
    #    bit 5 = supervisor timer interrupt  (scheduler tick)
    #    bit 9 = supervisor external interrupt (PLIC / devices)
    #    decimal: 2 + 32 + 512 = 546
    li t0, 546
    csrw mideleg, t0

    # 4a. Point mtvec at _m_trap (offset 0x100 = 256 from ROM base).
    li t0, 256
    csrw mtvec, t0

    # 4b. mstatus: set MPP=01 (Supervisor) so mret drops to S-mode.
    #     mstatus=0 at reset; bit 11 = MPP low bit → MPP=01.
    li t0, 1
    slli t0, t0, 11
    csrw mstatus, t0

    # 4c. mepc = kernel entry point.
    #     a0 is set to the ELF entry address by VirtualMachine::new_kernel
    #     before the CPU starts running, so _start never needs to know the
    #     actual address at ROM assembly time.
    csrw mepc, a0

    mret                        # drops to S-mode, jumps to kernel entry

    # Pad _start to exactly 0x100 bytes so _m_trap lands at offset 0x100.
    # _start is 64 bytes (15 instructions, with li 45312 expanding to lui+addi).
    # Padding = 256 - 64 = 192 bytes.
    .space 192

# ============================================================
# _m_trap: M-mode trap handler (offset 0x100)
#
# mtvec is set to 0x100 by Pipeline::new (for hosted programs)
# and by _start (for kernel mode) before mret.
#
# Handles:
#   cause  8 = ecall from U-mode  (hosted programs running in M-mode)
#   cause  9 = ecall from S-mode  (SBI calls from the kernel)
#   cause 11 = ecall from M-mode  (hosted programs running in M-mode)
#   anything else → mret (let the pipeline's trap logic deal with it)
# ============================================================
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

# sys_puts(ptr=a0) — null-terminated string + newline
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

# sys_exit(code=a0) — write to SYSCON, bus halts VM
sys_exit:
    li t0, 268500992
    sd a0, 0(t0)
    j _advance_mepc_and_mret

sys_unknown:
    li a0, -1
    j _advance_mepc_and_mret

# sys_printf(fmt=a0, a1..a6 = up to 6 args)
sys_printf:
    addi sp, sp, -48
    sd a1, 0(sp)
    sd a2, 8(sp)
    sd a3, 16(sp)
    sd a4, 24(sp)
    sd a5, 32(sp)
    sd a6, 40(sp)
    li t0, 268435456
    mv t1, a0
    li t2, 0
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

# %d / %i : signed decimal
_printf_fmt_d:
    bge t4, x0, _printf_fmt_u
    li t5, 45
    sb t5, 0(t0)
    sub t4, x0, t4
    j _printf_fmt_u

# %u : unsigned decimal
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

# %x / %X : lowercase hex
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
