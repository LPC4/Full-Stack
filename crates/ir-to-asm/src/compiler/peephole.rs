//! Conservative, basic-block-local peephole optimizer over the RvInstruction stream.
//!
//! Runs after lowering, just before the assembler consumes the tokens. All
//! reasoning is local to a straight-line run of real instructions: any label,
//! directive (which is also how branches, jumps, and calls are emitted here), or
//! pseudo-instruction ends the run and clears all tracking state, so control flow
//! is never reasoned across. Comments emit no code, so they are transparent for
//! matching and preserved in the output.
//!
//! The transforms target the load/store churn the per-vreg stack-slot lowering
//! leaves behind:
//!
//! - Drop a no-op self-move `addi rd, rd, 0` (this is how `mv rd, rd` is emitted).
//! - When a stack slot's current value is already known to live in a register
//!   (from the store or load that last touched it), fold a later reload of that
//!   slot: drop it if the value is already in the destination register, otherwise
//!   rewrite it as a register move and skip the memory access.
//!
//! Slots are tracked by their `sp`-relative byte offset. A slot address is
//! recognized either directly (`off(sp)`) or through a register materialized by
//! `addi rd, sp, K` (the lowering rematerializes slot addresses into temps before
//! every access). Only 64-bit `sd`/`ld` traffic is folded: sub-word reloads
//! re-extend the value and so are never treated as carrying the full register
//! value. Any write to `sp`, any store through an unresolved base, and any
//! unmodeled instruction conservatively discards the relevant tracking state.

use asm_to_binary::encode_decode::Reg;
use asm_to_binary::real::RealInstruction;
use asm_to_binary::riscv::rv64i::Addi;
use asm_to_binary::rv_instruction::RvInstruction;
use std::collections::HashMap;

const SP: Reg = 2;

/// What a real instruction does, as far as the tracker cares.
enum Effect {
    /// `addi rd, sp, k`: `rd` now equals `sp + k`.
    SpAddr { rd: Reg, k: i32 },
    /// `addi rd, rs, imm`: a general add-immediate (covers `mv` when `imm == 0`).
    Addi { rd: Reg, rs: Reg, imm: i32 },
    /// `ld rd, offset(base)`: a full 64-bit load.
    LoadFull { rd: Reg, base: Reg, offset: i32 },
    /// A sub-word load (`lw`/`lh`/`lb`/...): writes `rd`, carries no full value.
    LoadPart { rd: Reg },
    /// `sd src, offset(base)`: a full 64-bit store.
    StoreFull { src: Reg, base: Reg, offset: i32 },
    /// A sub-word store (`sw`/`sh`/`sb`): writes part of `offset(base)`.
    StorePart { base: Reg, offset: i32 },
    /// A plain integer-register write with no memory effect.
    WriteReg { rd: Reg },
    /// Anything not modeled: clears all tracking state.
    Opaque,
}

/// Straight-line tracking state, valid only within one run between barriers.
#[derive(Default)]
struct State {
    /// Registers known to equal `sp + k`.
    sp_addr: HashMap<Reg, i32>,
    /// `sp`-relative slot offset -> a register currently holding that slot's value.
    slot_reg: HashMap<i32, Reg>,
}

impl State {
    fn clear(&mut self) {
        self.sp_addr.clear();
        self.slot_reg.clear();
    }

    /// Record that `rd` is being overwritten, invalidating stale knowledge.
    /// Writing `sp` shifts every slot, so it clears everything.
    fn write_reg(&mut self, rd: Reg) {
        if rd == SP {
            self.clear();
            return;
        }
        self.sp_addr.remove(&rd);
        self.slot_reg.retain(|_, holder| *holder != rd);
    }

    /// Resolve a `(base, offset)` memory operand to an `sp`-relative slot offset.
    fn resolve_slot(&self, base: Reg, offset: i32) -> Option<i32> {
        if base == SP {
            Some(offset)
        } else {
            self.sp_addr.get(&base).map(|k| k + offset)
        }
    }
}

/// Apply the peephole transforms to a freshly lowered token stream.
pub fn optimize(tokens: &[RvInstruction]) -> Vec<RvInstruction> {
    let mut out: Vec<RvInstruction> = Vec::with_capacity(tokens.len());
    let mut state = State::default();

    for tok in tokens {
        let RvInstruction::Real(real) = tok else {
            match tok {
                // Comments carry no code, so they never disturb tracking.
                RvInstruction::Comment(_) => {}
                // Labels, directives, and pseudo-instructions end the run.
                _ => state.clear(),
            }
            out.push(tok.clone());
            continue;
        };

        match classify(real) {
            Effect::Addi { rd, rs, imm } if rd == rs && imm == 0 => {
                // Self-move: a no-op regardless of context.
            }
            Effect::Addi { rd, rs, imm } => {
                // Propagate `sp + k` knowledge through `addi rd, rs, imm`.
                let chained = self_sp_offset(&state, rs, imm);
                state.write_reg(rd);
                if let Some(k) = chained {
                    state.sp_addr.insert(rd, k);
                }
                out.push(tok.clone());
            }
            Effect::SpAddr { rd, k } => {
                if rd == SP {
                    state.clear();
                } else {
                    state.write_reg(rd);
                    state.sp_addr.insert(rd, k);
                }
                out.push(tok.clone());
            }
            Effect::LoadFull { rd, base, offset } => {
                if let Some(slot) = state.resolve_slot(base, offset)
                    && let Some(holder) = state.slot_reg.get(&slot).copied()
                {
                    if holder == rd {
                        // Value already in the destination register: drop the load.
                        continue;
                    }
                    // Value still live in `holder`: move instead of reloading.
                    state.write_reg(rd);
                    out.push(RvInstruction::Real(RealInstruction::Addi(Addi::new(
                        rd, holder, 0,
                    ))));
                    continue;
                }
                // Establishes that `rd` now holds this slot's value.
                let slot = state.resolve_slot(base, offset);
                state.write_reg(rd);
                if let Some(slot) = slot {
                    state.slot_reg.insert(slot, rd);
                }
                out.push(tok.clone());
            }
            Effect::LoadPart { rd } => {
                state.write_reg(rd);
                out.push(tok.clone());
            }
            Effect::StoreFull { src, base, offset } => {
                match state.resolve_slot(base, offset) {
                    Some(slot) => {
                        // `src` now holds this slot's value.
                        state.slot_reg.insert(slot, src);
                    }
                    None => {
                        // Unknown destination may alias a tracked slot.
                        state.slot_reg.clear();
                    }
                }
                out.push(tok.clone());
            }
            Effect::StorePart { base, offset } => {
                match state.resolve_slot(base, offset) {
                    Some(slot) => {
                        // Partial write: the full register value no longer matches.
                        state.slot_reg.remove(&slot);
                    }
                    None => state.slot_reg.clear(),
                }
                out.push(tok.clone());
            }
            Effect::WriteReg { rd } => {
                state.write_reg(rd);
                out.push(tok.clone());
            }
            Effect::Opaque => {
                state.clear();
                out.push(tok.clone());
            }
        }
    }

    out
}

/// `sp + (k + imm)` if `rs` is known to equal `sp + k` (or is `sp` itself).
fn self_sp_offset(state: &State, rs: Reg, imm: i32) -> Option<i32> {
    if rs == SP {
        Some(imm)
    } else {
        state.sp_addr.get(&rs).map(|k| k + imm)
    }
}

fn classify(real: &RealInstruction) -> Effect {
    use RealInstruction as R;
    match real {
        R::Addi(a) if a.rs1 == SP => Effect::SpAddr { rd: a.rd, k: a.imm },
        R::Addi(a) => Effect::Addi {
            rd: a.rd,
            rs: a.rs1,
            imm: a.imm,
        },

        R::Ld(l) => Effect::LoadFull {
            rd: l.rd,
            base: l.base,
            offset: l.offset,
        },
        R::Lw(l) => Effect::LoadPart { rd: l.rd },
        R::Lh(l) => Effect::LoadPart { rd: l.rd },
        R::Lb(l) => Effect::LoadPart { rd: l.rd },
        R::Lwu(l) => Effect::LoadPart { rd: l.rd },
        R::Lhu(l) => Effect::LoadPart { rd: l.rd },
        R::Lbu(l) => Effect::LoadPart { rd: l.rd },

        R::Sd(s) => Effect::StoreFull {
            src: s.src,
            base: s.base,
            offset: s.offset,
        },
        R::Sw(s) => Effect::StorePart {
            base: s.base,
            offset: s.offset,
        },
        R::Sh(s) => Effect::StorePart {
            base: s.base,
            offset: s.offset,
        },
        R::Sb(s) => Effect::StorePart {
            base: s.base,
            offset: s.offset,
        },

        // Integer-register-writing ALU ops the lowering emits. These carry no new
        // slot knowledge; they only invalidate their destination register.
        R::Add(i) => Effect::WriteReg { rd: i.rd },
        R::Sub(i) => Effect::WriteReg { rd: i.rd },
        R::Mul(i) => Effect::WriteReg { rd: i.rd },
        R::Div(i) => Effect::WriteReg { rd: i.rd },
        R::Rem(i) => Effect::WriteReg { rd: i.rd },
        R::Divu(i) => Effect::WriteReg { rd: i.rd },
        R::Remu(i) => Effect::WriteReg { rd: i.rd },
        R::And(i) => Effect::WriteReg { rd: i.rd },
        R::Or(i) => Effect::WriteReg { rd: i.rd },
        R::Xor(i) => Effect::WriteReg { rd: i.rd },
        R::Xori(i) => Effect::WriteReg { rd: i.rd },
        R::Sltiu(i) => Effect::WriteReg { rd: i.rd },
        R::Sltu(i) => Effect::WriteReg { rd: i.rd },
        R::Slt(i) => Effect::WriteReg { rd: i.rd },
        R::Sll(i) => Effect::WriteReg { rd: i.rd },
        R::Srl(i) => Effect::WriteReg { rd: i.rd },
        R::Slli(i) => Effect::WriteReg { rd: i.rd },
        R::Srli(i) => Effect::WriteReg { rd: i.rd },
        R::Srai(i) => Effect::WriteReg { rd: i.rd },
        R::Addiw(i) => Effect::WriteReg { rd: i.rd },
        R::Lui(i) => Effect::WriteReg { rd: i.rd },

        // Calls, jumps, system, FP, CSR, and anything else: discard all state.
        _ => Effect::Opaque,
    }
}

#[cfg(test)]
mod tests {
    use super::optimize;
    use asm_to_binary::real::RealInstruction;
    use asm_to_binary::riscv::rv64i::{Add, Addi, Ld, Lw, Sd, Sw};
    use asm_to_binary::rv_instruction::RvInstruction;

    fn real(inst: RealInstruction) -> RvInstruction {
        RvInstruction::Real(inst)
    }

    fn reals(tokens: &[RvInstruction]) -> Vec<RealInstruction> {
        tokens
            .iter()
            .filter_map(|t| match t {
                RvInstruction::Real(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn drops_self_move() {
        // addi t0, t0, 0  (mv t0, t0) is a no-op and must vanish.
        let input = vec![
            real(RealInstruction::Addi(Addi::new(5, 5, 0))),
            real(RealInstruction::Add(Add::new(6, 7, 8))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 1, "self-move should be dropped");
        assert!(matches!(&out[0], RealInstruction::Add(_)));
    }

    #[test]
    fn keeps_real_move() {
        // addi t0, t1, 0 (mv t0, t1) moves between distinct registers; keep it.
        let input = vec![real(RealInstruction::Addi(Addi::new(5, 6, 0)))];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 1, "a move between distinct registers is kept");
    }

    #[test]
    fn drops_reload_into_same_register() {
        // sd t0, 16(sp) ; ld t0, 16(sp) -> the ld is redundant.
        let input = vec![
            real(RealInstruction::Sd(Sd::new(2, 5, 16))),
            real(RealInstruction::Ld(Ld::new(5, 2, 16))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 1, "redundant reload should be dropped");
        assert!(matches!(&out[0], RealInstruction::Sd(_)));
    }

    #[test]
    fn rewrites_reload_into_other_register_as_move() {
        // sd t0, 16(sp) ; ld t1, 16(sp) -> sd ... ; mv t1, t0
        let input = vec![
            real(RealInstruction::Sd(Sd::new(2, 5, 16))),
            real(RealInstruction::Ld(Ld::new(6, 2, 16))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 2);
        assert!(matches!(&out[0], RealInstruction::Sd(_)));
        match &out[1] {
            RealInstruction::Addi(a) => {
                assert_eq!((a.rd, a.rs1, a.imm), (6, 5, 0), "expected mv t1, t0");
            }
            other => panic!("expected reload rewritten to a move, got {other:?}"),
        }
    }

    #[test]
    fn folds_through_materialized_slot_address() {
        // The lowering rematerializes a slot address into a temp before each
        // access:
        //   addi t0, sp, 8 ; sd t1, 0(t0) ; addi t0, sp, 8 ; ld t2, 0(t0)
        // Both `0(t0)` resolve to slot sp+8, so the reload becomes `mv t2, t1`.
        let input = vec![
            real(RealInstruction::Addi(Addi::new(5, 2, 8))), // addi t0, sp, 8
            real(RealInstruction::Sd(Sd::new(5, 6, 0))),     // sd t1, 0(t0)
            real(RealInstruction::Addi(Addi::new(5, 2, 8))), // addi t0, sp, 8
            real(RealInstruction::Ld(Ld::new(7, 5, 0))),     // ld t2, 0(t0)
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 4, "address recompute + store + mv");
        match out.last().unwrap() {
            RealInstruction::Addi(a) => {
                assert_eq!((a.rd, a.rs1, a.imm), (7, 6, 0), "expected mv t2, t1");
            }
            other => panic!("expected reload rewritten to a move, got {other:?}"),
        }
    }

    #[test]
    fn reload_after_holder_is_clobbered_is_kept() {
        // sd t0, 8(sp) ; addi t0, zero, 1 (clobbers t0) ; ld t1, 8(sp)
        // The holder no longer holds the slot value, so the load must stay.
        let input = vec![
            real(RealInstruction::Sd(Sd::new(2, 5, 8))),
            real(RealInstruction::Addi(Addi::new(5, 0, 1))),
            real(RealInstruction::Ld(Ld::new(6, 2, 8))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 3, "reload kept after holder is clobbered");
        assert!(matches!(out.last().unwrap(), RealInstruction::Ld(_)));
    }

    #[test]
    fn comment_between_store_and_load_is_transparent() {
        let input = vec![
            real(RealInstruction::Sd(Sd::new(2, 5, 8))),
            RvInstruction::Comment("+ operation on i64".to_owned()),
            real(RealInstruction::Ld(Ld::new(5, 2, 8))),
        ];
        let out = optimize(&input);
        assert!(out.iter().any(|t| matches!(t, RvInstruction::Comment(_))));
        assert_eq!(reals(&out).len(), 1, "reload dropped across a comment");
    }

    #[test]
    fn does_not_fold_across_a_label() {
        // A label is a run barrier: the load may be a branch target reached with a
        // different value in the register.
        let input = vec![
            real(RealInstruction::Sd(Sd::new(2, 5, 8))),
            RvInstruction::Label("L1".to_owned()),
            real(RealInstruction::Ld(Ld::new(5, 2, 8))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 2, "fold must not cross a label");
    }

    #[test]
    fn does_not_fold_across_a_directive() {
        // Branches/jumps/calls are emitted as directives; they end the run.
        let input = vec![
            real(RealInstruction::Sd(Sd::new(2, 5, 8))),
            RvInstruction::Directive("\tj some_label".to_owned()),
            real(RealInstruction::Ld(Ld::new(5, 2, 8))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 2, "fold must not cross a directive");
    }

    #[test]
    fn does_not_fold_non_sp_base() {
        // sd t0, 0(t1) ; ld t0, 0(t1): t1 is not sp-derived, so leave it alone.
        let input = vec![
            real(RealInstruction::Sd(Sd::new(6, 5, 0))),
            real(RealInstruction::Ld(Ld::new(5, 6, 0))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 2, "non-sp store/load must not be folded");
    }

    #[test]
    fn does_not_fold_subword_store_load() {
        // sw/lw can re-extend differently from the stored 64-bit register.
        let input = vec![
            real(RealInstruction::Sw(Sw::new(2, 5, 8))),
            real(RealInstruction::Lw(Lw::new(5, 2, 8))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 2, "sub-word store/load must not be folded");
    }

    #[test]
    fn subword_store_invalidates_slot() {
        // sd t0, 8(sp) ; sw t2, 8(sp) ; ld t1, 8(sp)
        // The sub-word store partially overwrites the slot, so the reload stays.
        let input = vec![
            real(RealInstruction::Sd(Sd::new(2, 5, 8))),
            real(RealInstruction::Sw(Sw::new(2, 7, 8))),
            real(RealInstruction::Ld(Ld::new(6, 2, 8))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 3, "reload kept after a partial overwrite");
        assert!(matches!(out.last().unwrap(), RealInstruction::Ld(_)));
    }

    #[test]
    fn does_not_fold_mismatched_offset() {
        let input = vec![
            real(RealInstruction::Sd(Sd::new(2, 5, 8))),
            real(RealInstruction::Ld(Ld::new(5, 2, 16))),
        ];
        let out = reals(&optimize(&input));
        assert_eq!(out.len(), 2, "different slots must not be folded");
    }
}
