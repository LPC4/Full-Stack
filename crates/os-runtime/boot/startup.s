# ROM firmware, M-mode startup

.section .text
    .globl _start
    .globl _m_trap

# _start: M-mode boot stub (offset 0x000)
#
# 1. PMP     - grant S/U-mode RWX access to full address space
# 2. medeleg - delegate page faults and U-mode ecall to S-mode
# 3. mideleg - delegate supervisor interrupts to S-mode
# 4. mret into S-mode at kernel entry (address in a0)
_start:
    # PMP: single entry, full address space, RWX.
    #    pmpaddr0 = -1  (TOR upper bound = all ones)
    #    pmpcfg0  = 0x1F (A=NAPOT, X=1, W=1, R=1)
    li t0, -1
    csrw pmpaddr0, t0
    li t0, 31
    csrw pmpcfg0, t0

    # medeleg: delegate to S-mode.
    #    bit  8 = ecall from U-mode
    #    bit 12 = instruction page fault
    #    bit 13 = load page fault
    #    bit 15 = store page fault
    li t0, 45312
    csrw medeleg, t0

    # mideleg: delegate to S-mode.
    #    bit 1 = supervisor software interrupt
    #    bit 5 = supervisor timer interrupt
    #    bit 9 = supervisor external interrupt (PLIC)
    li t0, 546
    csrw mideleg, t0

    # mtvec = _m_trap (offset 0x100 = 256 from ROM base)
    li t0, 256
    csrw mtvec, t0

    # mstatus: MPP=01 (Supervisor) so mret drops to S-mode.
    #     bit 11 = MPP low bit
    li t0, 1
    slli t0, t0, 11
    csrw mstatus, t0

    # mepc = kernel entry point (passed in a0 by VirtualMachine::new_kernel)
    csrw mepc, a0

    mret

    # Pad to 0x100 bytes. _start is 64 bytes (15 insns; li 45312 expands to lui+addi).
    # 256 - 64 = 192 bytes padding.
    .space 192
