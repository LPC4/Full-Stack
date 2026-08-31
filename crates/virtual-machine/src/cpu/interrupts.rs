//! Architectural interrupt input sampling and priority selection.

use crate::bus::SystemBus;
use crate::cpu::ArchitecturalState;
use crate::cpu::registers::PrivilegeMode;

pub(crate) fn sample_inputs(state: &mut ArchitecturalState, bus: &mut SystemBus) {
    {
        let clint = bus.clint_mut();
        clint.tick();
        let csrs = state.csrs_mut();
        if clint.timer_irq_pending() {
            if (csrs.mideleg >> 5) & 1 == 1 {
                csrs.mip |= 1u64 << 5;
                csrs.mip &= !(1u64 << 7);
            } else {
                csrs.mip |= 1u64 << 7;
            }
        } else {
            csrs.mip &= !(1u64 << 7);
            csrs.mip &= !(1u64 << 5);
        }
        if clint.software_irq_pending() {
            csrs.mip |= 1u64 << 3;
        } else {
            csrs.mip &= !(1u64 << 3);
        }
    }

    if bus.uart_mut().rx_irq_pending() {
        bus.plic_mut()
            .set_irq(crate::devices::uart::UART_RX_IRQ_SOURCE);
    }

    let csrs = state.csrs_mut();
    if bus.plic_mut().next_irq(0).is_some() {
        if (csrs.mideleg >> 9) & 1 == 1 {
            csrs.mip |= 1u64 << 9;
        } else {
            csrs.mip |= 1u64 << 11;
        }
    } else {
        csrs.mip &= !(1u64 << 11);
        csrs.mip &= !(1u64 << 9);
    }
}

pub(crate) fn pending(state: &ArchitecturalState) -> Option<(u64, u64, u64)> {
    let csrs = state.csrs();
    let pending = csrs.mip & csrs.mie;
    if pending == 0 {
        return None;
    }

    let privilege = state.privilege_mode();
    let in_machine = privilege == PrivilegeMode::Machine;
    let in_supervisor = privilege == PrivilegeMode::Supervisor;
    let machine_enabled = (csrs.mstatus >> 3) & 1 == 1;
    let supervisor_enabled = (csrs.mstatus >> 1) & 1 == 1;

    let machine_pending = pending & !csrs.mideleg;
    if machine_pending != 0 && (!in_machine || machine_enabled) {
        let cause = prioritized(machine_pending, &[11, 3, 7]);
        return Some(((1u64 << 63) | cause, 0, state.pc()));
    }

    let supervisor_pending = pending & csrs.mideleg;
    if supervisor_pending != 0 && !in_machine && (!in_supervisor || supervisor_enabled) {
        let cause = prioritized(supervisor_pending, &[9, 1, 5]);
        return Some(((1u64 << 63) | cause, 0, state.pc()));
    }

    None
}

fn prioritized(pending: u64, priorities: &[u64]) -> u64 {
    priorities
        .iter()
        .copied()
        .find(|cause| pending & (1 << cause) != 0)
        .unwrap_or_else(|| u64::from(pending.trailing_zeros()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_interrupt_priority_is_stable() {
        let mut state = ArchitecturalState::new(0x8000_0000, 0x8800_0000);
        let csrs = state.csrs_mut();
        csrs.mstatus |= 1 << 3;
        csrs.mie = (1 << 11) | (1 << 7) | (1 << 3);
        csrs.mip = csrs.mie;

        assert_eq!(pending(&state), Some(((1u64 << 63) | 11, 0, 0x8000_0000)));
    }

    #[test]
    fn delegated_supervisor_interrupt_priority_is_stable() {
        let mut state = ArchitecturalState::new(0x8000_0000, 0x8800_0000);
        state.registers_mut().priv_mode = PrivilegeMode::Supervisor;
        let csrs = state.csrs_mut();
        csrs.mstatus |= 1 << 1;
        csrs.mie = (1 << 9) | (1 << 5) | (1 << 1);
        csrs.mip = csrs.mie;
        csrs.mideleg = csrs.mie;

        assert_eq!(pending(&state), Some(((1u64 << 63) | 9, 0, 0x8000_0000)));
    }
}
