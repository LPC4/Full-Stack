#![allow(non_snake_case)]

use asm_to_binary::*;
use asm_to_binary::encode_decode::*;
use asm_to_binary::traits::Instruction;
use asm_to_binary::pseudo::PseudoInstruction;
use asm_to_binary::real::RealInstruction;
use asm_to_binary::riscv::rv64a::*;
use asm_to_binary::riscv::rv64fd::*;
use asm_to_binary::riscv::rv64i::*;
use asm_to_binary::riscv::rv64m::*;
use asm_to_binary::riscv::rv64zicsr::*;
use asm_to_binary::utils::reg_name;

macro_rules! assert_enc {
    ($instr:expr, $expected:expr) => {{
        let i = $instr;
        let enc = i.encode();
        assert_eq!(
            enc, $expected,
            "Encoding mismatch for `{}`\n  got      0x{:08x}\n  expected 0x{:08x}",
            i.to_asm(), enc, $expected
        );
    }};
}

macro_rules! assert_pseudo_expands_to {
    ($pseudo:expr, $expected:expr) => {{
        let expanded = $pseudo.expand();
        let expected: Vec<RealInstruction> = $expected;
        assert_eq!(
            expanded.len(),
            expected.len(),
            "Expansion length mismatch for `{}`",
            $pseudo.to_asm()
        );
        for (got, exp) in expanded.iter().zip(expected.iter()) {
            assert_eq!(
                got.encode(),
                exp.encode(),
                "Expansion mismatch for `{}`\n  got      {}\n  expected {}",
                $pseudo.to_asm(),
                got.to_asm(),
                exp.to_asm()
            );
        }
    }};
}

// ---------------------------------------------------------------------------
// RV64I - Register‑register
// ---------------------------------------------------------------------------

#[test]
fn rv64i_r_type_add() {
    assert_enc!(Add::new(1, 2, 3),   0x003100b3u32);
    assert_enc!(Sub::new(4, 5, 6),   0x40628233u32);
    assert_enc!(Sll::new(7, 8, 9),   0x009413b3u32);
    assert_enc!(Slt::new(10, 11, 12),0x00c5a533u32);
    assert_enc!(Sltu::new(13, 14, 15),0x00f736b3u32);
    assert_enc!(Xor::new(16, 17, 18),0x0128c833u32);
    assert_enc!(Srl::new(19, 20, 21),0x015a59b3u32);
    assert_enc!(Sra::new(22, 23, 24),0x418bdb33u32);
    assert_enc!(Or::new(25, 26, 27), 0x01bd6cb3u32);
    assert_enc!(And::new(28, 29, 30),0x01eefe33u32);
}

#[test]
fn rv64i_r_type_w() {
    assert_enc!(Addw::new(1, 2, 3), 0x003100bbu32);
    assert_enc!(Subw::new(4, 5, 6), 0x4062823bu32);
}

// ---------------------------------------------------------------------------
// RV64I - Immediate ALU
// ---------------------------------------------------------------------------

#[test]
fn rv64i_i_imm() {
    assert_enc!(Addi::new(1, 2, 42),  0x02a10093u32);
    assert_enc!(Slti::new(3, 4, -1),  0xfff22193u32);
    assert_enc!(Sltiu::new(5, 6, 1),  0x00133293u32);
    assert_enc!(Xori::new(7, 8, 0xFF),0x0ff44393u32);
    assert_enc!(Ori::new(9, 10, 0),   0x00056493u32);
    assert_enc!(Andi::new(11, 12, 0x7FF), 0x7ff67593u32);
}

#[test]
fn rv64i_shift_imm() {
    assert_enc!(Slli::new(10, 11, 5),  0x00559513u32);
    assert_enc!(Srli::new(10, 11, 5),  0x0055d513u32);
    assert_enc!(Srai::new(10, 11, 5),  0x4055d513u32);
    assert_enc!(Slli::new(10, 11, 63), 0x03f59513u32);
}

#[test]
fn rv64i_w_shift_imm() {
    assert_enc!(Slliw::new(10, 11, 4), 0x0045951bu32);
    assert_enc!(Srliw::new(10, 11, 4), 0x0045d51bu32);
    assert_enc!(Sraiw::new(10, 11, 4), 0x4045d51bu32);
}

// ---------------------------------------------------------------------------
// RV64I - Loads / Stores
// ---------------------------------------------------------------------------

#[test]
fn rv64i_loads() {
    assert_enc!(Lb::new(5, 6, 0),    0x00030283u32);
    assert_enc!(Lh::new(5, 6, 4),    0x00431283u32);
    assert_enc!(Lw::new(5, 6, -8),   0xff832283u32);
    assert_enc!(Ld::new(5, 6, 2047), 0x7ff33283u32);
    assert_enc!(Lbu::new(5, 6, 128), 0x08034283u32);
    assert_enc!(Lhu::new(5, 6, 128), 0x08035283u32);
    assert_enc!(Lwu::new(5, 6, 128), 0x08036283u32);
}

#[test]
fn rv64i_stores() {
    assert_enc!(Sb::new(6, 5, 0),    0x00530023u32);
    assert_enc!(Sh::new(6, 5, 4),    0x00531223u32);
    assert_enc!(Sw::new(6, 5, -4),   0xfe532e23u32);
    assert_enc!(Sd::new(6, 5, 2047), 0x7e533fa3u32);
}

// ---------------------------------------------------------------------------
// RV64I - Branches / Jump
// ---------------------------------------------------------------------------

#[test]
fn rv64i_branches() {
    assert_enc!(Beq::new(5, 6, 4),     0x00628263u32);
    assert_enc!(Bne::new(5, 6, -4),    0xfe629ee3u32);
    assert_enc!(Blt::new(5, 6, 4094),  0x7e62cfe3u32);
    assert_enc!(Bge::new(5, 6, -4096), 0x8062d063u32);
}

#[test]
fn rv64i_jump() {
    assert_enc!(Jal::new(1, 4),   0x004000efu32);
    assert_enc!(Jal::new(0, -8),  0xff9ff06fu32);
    assert_enc!(Jalr::new(1, 5, 0), 0x000280e7u32);
}

// ---------------------------------------------------------------------------
// RV64I - Upper immediate
// ---------------------------------------------------------------------------

#[test]
fn rv64i_upper() {
    assert_enc!(Lui::new(5, 0x12345000),  0x123452b7u32);
    assert_enc!(Auipc::new(5, 0xABCDE000u32 as i32), 0xabcde297u32);
}

// ---------------------------------------------------------------------------
// RV64I - System / fence
// ---------------------------------------------------------------------------

#[test]
fn rv64i_system() {
    assert_enc!(Ecall::new(),     0x00000073u32);
    assert_enc!(Ebreak::new(),    0x00100073u32);
    assert_enc!(Fence::new(0b1100, 0b0011), 0x0c30000fu32);
    assert_enc!(FenceI::new(),    0x0000100fu32);
}

// ---------------------------------------------------------------------------
// RV64M
// ---------------------------------------------------------------------------

#[test]
fn rv64m() {
    assert_enc!(Mul::new(10, 11, 12),  0x02c58533u32);
    assert_enc!(Div::new(10, 11, 12),  0x02c5c533u32);
    assert_enc!(Divu::new(10, 11, 12), 0x02c5d533u32);
    assert_enc!(Rem::new(10, 11, 12),  0x02c5e533u32);
    assert_enc!(Remu::new(10, 11, 12), 0x02c5f533u32);
    assert_enc!(Mulw::new(10, 11, 12), 0x02c5853bu32);
    assert_enc!(Divw::new(10, 11, 12), 0x02c5c53bu32);
}

// ---------------------------------------------------------------------------
// RV64A - Atomics
// ---------------------------------------------------------------------------

#[test]
fn rv64a_amos() {
    assert_enc!(AmoaddW::new(1, 2, 3), 0x003120afu32);
    assert_enc!(AmoswapD::new(4, 5, 6).with_ordering(Ordering::aq()), 0x0c62b22fu32);
    assert_enc!(AmoxorW::new(7, 8, 9).with_ordering(Ordering::aqrl()), 0x269423afu32);
}

#[test]
fn rv64a_lr_sc() {
    assert_enc!(Lr::w(10, 11), 0x1005a52fu32);
    assert_enc!(Lr::d(10, 11).with_ordering(Ordering::rl()), 0x1205b52fu32);
    assert_enc!(Sc::w(12, 13, 14).with_ordering(Ordering::aq()), 0x1ce6a62fu32);
    assert_enc!(Sc::d(0, 5, 4).with_ordering(Ordering::aqrl()), 0x1e42b02fu32);
}

// ---------------------------------------------------------------------------
// RV64FD - FP loads/stores
// ---------------------------------------------------------------------------

#[test]
fn rv64fd_loads() {
    assert_enc!(Flw::new(5, 6, 0),  0x00032287u32);
    assert_enc!(Fld::new(5, 6, 16), 0x01033287u32);
    assert_enc!(Fsw::new(6, 5, 0),  0x00532027u32);
    assert_enc!(Fsd::new(6, 5, -8), 0xfe533c27u32);
}

// ---------------------------------------------------------------------------
// RV64FD - FP ALU
// ---------------------------------------------------------------------------

#[test]
fn rv64fd_alu() {
    assert_enc!(Fadd::new(10, 11, 12),    0x00c58553u32);
    assert_enc!(Fsub::new(10, 11, 12),    0x08c58553u32);
    assert_enc!(Fmul::new(10, 11, 12),    0x10c58553u32);
    assert_enc!(Fdiv::new(10, 11, 12),    0x18c58553u32);
    assert_enc!(FsqrtS::new(10, 11),      0x58058553u32);
    assert_enc!(Fsgnj::new(10, 11, 12),   0x20c58553u32);
    assert_enc!(Fsgnjn::new(10, 11, 12),  0x20c59553u32);
    assert_enc!(Fsgnjx::new(10, 11, 12),  0x20c5a553u32);
    assert_enc!(Fmin::new(10, 11, 12),    0x28c58553u32);
    assert_enc!(Fmax::new(10, 11, 12),    0x28c59553u32);
}

#[test]
fn rv64fd_alu_double() {
    assert_enc!(FaddD::new(10, 11, 12),   0x02c58553u32);
    assert_enc!(FsubD::new(10, 11, 12),   0x0ac58553u32);
    assert_enc!(FmulD::new(10, 11, 12),   0x12c58553u32);
    assert_enc!(FdivD::new(10, 11, 12),   0x1ac58553u32);
    assert_enc!(FsqrtD::new(10, 11),      0x5a058553u32);
    assert_enc!(FsgnjD::new(10, 11, 12),  0x22c58553u32);
}

// ---------------------------------------------------------------------------
// RV64FD - FP compare / classify
// ---------------------------------------------------------------------------

#[test]
fn rv64fd_compare() {
    assert_enc!(FeqS::new(10, 11, 12),  0xa0c5a553u32);
    assert_enc!(FltS::new(10, 11, 12),  0xa0c59553u32);
    assert_enc!(FleqS::new(10, 11, 12), 0xa0c58553u32);
    assert_enc!(FeqD::new(10, 11, 12),  0xa2c5a553u32);
    assert_enc!(FltD::new(10, 11, 12),  0xa2c59553u32);
    assert_enc!(FleqD::new(10, 11, 12), 0xa2c58553u32);
}

#[test]
fn rv64fd_fclass() {
    assert_enc!(FclassS::new(10, 11), 0xe0059553u32);
    assert_enc!(FclassD::new(10, 11), 0xe2059553u32);
}

// ---------------------------------------------------------------------------
// RV64FD - FP move (bitwise)
// ---------------------------------------------------------------------------

#[test]
fn rv64fd_move() {
    assert_enc!(FmvXW::new(10, 20), 0xe00a0553u32);
    assert_enc!(FmvWX::new(10, 5),  0xf0028553u32);
    assert_enc!(FmvXD::new(10, 20), 0xe20a0553u32);
    assert_enc!(FmvDX::new(10, 5),  0xf2028553u32);
}

// ---------------------------------------------------------------------------
// RV64FD - FP ↔ int conversions
// ---------------------------------------------------------------------------

#[test]
fn rv64fd_float_to_int() {
    assert_enc!(FcvtWS::new(10, 20),  0xc00a0553u32);
    assert_enc!(FcvtWUS::new(10, 20), 0xc01a0553u32);
    assert_enc!(FcvtLS::new(10, 20),  0xc02a0553u32);
    assert_enc!(FcvtLUS::new(10, 20), 0xc03a0553u32);
    assert_enc!(FcvtWD::new(10, 20),  0xc20a0553u32);
    assert_enc!(FcvtWUD::new(10, 20), 0xc21a0553u32);
    assert_enc!(FcvtLD::new(10, 20),  0xc22a0553u32);
    assert_enc!(FcvtLUD::new(10, 20), 0xc23a0553u32);
}

#[test]
fn rv64fd_int_to_float() {
    assert_enc!(FcvtSW::new(10, 5),  0xd0028553u32);
    assert_enc!(FcvtSWU::new(10, 5), 0xd0128553u32);
    assert_enc!(FcvtSL::new(10, 5),  0xd0228553u32);
    assert_enc!(FcvtSLU::new(10, 5), 0xd0328553u32);
    assert_enc!(FcvtDW::new(10, 5),  0xd2028553u32);
    assert_enc!(FcvtDL::new(10, 5),  0xd2228553u32);
}

#[test]
fn rv64fd_float_to_float() {
    assert_enc!(FcvtSD::new(10, 20), 0x401a0553u32);  // double -> single
    assert_enc!(FcvtDS::new(10, 20), 0x4a0a0553u32);  // single -> double
}

// ---------------------------------------------------------------------------
// RV64FD - FMAC (R4-type)
// ---------------------------------------------------------------------------

#[test]
fn rv64fd_fmac() {
    assert_enc!(FmaddS::new(10, 11, 12, 13),  0x68c58543u32);
    assert_enc!(FmsubD::new(10, 11, 12, 13),  0x6ac58547u32);
    assert_enc!(FnmsubS::new(10, 11, 12, 13), 0x68c5854bu32);
    assert_enc!(FnmaddD::new(10, 11, 12, 13), 0x6ac5854fu32);
}

// ---------------------------------------------------------------------------
// RV64Zicsr - CSR instructions
// ---------------------------------------------------------------------------

#[test]
fn rv64zicsr_reg() {
    assert_enc!(Csrrw::new(10, 0x300, 5), 0x30029573u32);
    assert_enc!(Csrrs::new(5, 0x001, 0),  0x001022f3u32);
    assert_enc!(Csrrc::new(5, 0x001, 0),  0x001032f3u32);
}

#[test]
fn rv64zicsr_imm() {
    assert_enc!(Csrrwi::new(10, 0x300, 5), 0x3002d573u32);
    assert_enc!(Csrrsi::new(5, 0x001, 0),  0x001062f3u32);
    assert_enc!(Csrrci::new(5, 0x001, 0),  0x001072f3u32);
}

// ---------------------------------------------------------------------------
// RealInstruction enum dispatching
// ---------------------------------------------------------------------------

#[test]
fn real_inst_enum_encoding() {
    let i = RealInstruction::Add(Add::new(1, 2, 3));
    assert_eq!(i.encode(), 0x003100b3u32);
    let i = RealInstruction::Lb(Lb::new(5, 6, 0));
    assert_eq!(i.encode(), 0x00030283u32);
    let i = RealInstruction::FaddD(FaddD::new(10, 11, 12));
    assert_eq!(i.encode(), 0x02c58553u32);
}

// ---------------------------------------------------------------------------
// Pseudo‑instruction expansions
// ---------------------------------------------------------------------------

#[test]
fn pseudo_nop() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Nop,
        vec![RealInstruction::Addi(Addi::new(0, 0, 0))]
    );
}

#[test]
fn pseudo_mv() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Mv { rd: 10, rs: 11 },
        vec![RealInstruction::Addi(Addi::new(10, 11, 0))]
    );
}

#[test]
fn pseudo_not() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Not { rd: 10, rs: 11 },
        vec![RealInstruction::Xori(Xori::new(10, 11, -1))]
    );
}

#[test]
fn pseudo_li_small() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Li { rd: 5, imm: 42 },
        vec![RealInstruction::Addi(Addi::new(5, 0, 42))]
    );
}

#[test]
fn pseudo_li_32bit() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Li { rd: 5, imm: 0x12345 },
        vec![
            RealInstruction::Lui(Lui::new(5, 0x12000)),
            RealInstruction::Addi(Addi::new(5, 5, 0x345)),
        ]
    );
}

#[test]
fn pseudo_li_64bit() {
    let imm: i64 = 0x1234_5678_9ABC;
    let expanded = PseudoInstruction::Li { rd: 5, imm }.expand();
    assert_eq!(expanded.len(), 8);
}

#[test]
fn pseudo_beqz() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Beqz { rs: 5, offset: 4 },
        vec![RealInstruction::Beq(Beq::new(5, 0, 4))]
    );
}

#[test]
fn pseudo_bgt() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Bgt { rs1: 5, rs2: 6, offset: 4 },
        vec![RealInstruction::Blt(Blt::new(6, 5, 4))]
    );
}

#[test]
fn pseudo_bgtu() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Bgtu { rs1: 5, rs2: 6, offset: 4 },
        vec![RealInstruction::Bltu(Bltu::new(6, 5, 4))]
    );
}

#[test]
fn pseudo_j() {
    assert_pseudo_expands_to!(
        PseudoInstruction::J { offset: 4 },
        vec![RealInstruction::Jal(Jal::new(0, 4))]
    );
}

#[test]
fn pseudo_ret() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Ret,
        vec![RealInstruction::Jalr(Jalr::new(0, 1, 0))]
    );
}

#[test]
fn pseudo_call() {
    assert_pseudo_expands_to!(
        PseudoInstruction::Call { symbol: "foo".into() },
        vec![
            RealInstruction::Auipc(Auipc::new(1, 0)),
            RealInstruction::Jalr(Jalr::new(1, 1, 0)),
        ]
    );
}

#[test]
fn pseudo_fp_mv_neg_abs() {
    assert_pseudo_expands_to!(
        PseudoInstruction::FmvS { fd: 10, fs: 11 },
        vec![RealInstruction::Fsgnj(Fsgnj::new(10, 11, 11))]
    );
    assert_pseudo_expands_to!(
        PseudoInstruction::FnegS { fd: 10, fs: 11 },
        vec![RealInstruction::Fsgnjn(Fsgnjn::new(10, 11, 11))]
    );
    assert_pseudo_expands_to!(
        PseudoInstruction::FabsS { fd: 10, fs: 11 },
        vec![RealInstruction::Fsgnjx(Fsgnjx::new(10, 11, 11))]
    );
}

// ---------------------------------------------------------------------------
// Validation / panic tests
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "offset")]
fn branch_odd_offset() {
    Beq::new(1, 2, 3);
}

#[test]
#[should_panic(expected = "offset")]
fn branch_too_large() {
    Beq::new(1, 2, 4096);
}

#[test]
#[should_panic(expected = "shift amount")]
fn slli_shamt_64() {
    Slli::new(1, 2, 64);
}

#[test]
#[should_panic(expected = "shift amount")]
fn slliw_shamt_64() {
    Slliw::new(1, 2, 64);
}

#[test]
#[should_panic(expected = "CSR address")]
fn csr_address_too_large() {
    Csrrw::new(1, 0x1000, 2);
}

#[test]
#[should_panic(expected = "rounding mode")]
fn bad_fp_rounding_mode() {
    Fadd::new(10, 11, 12).with_rm(5);
}

#[test]
#[should_panic(expected = "immediate")]
fn addi_imm_out_of_range() {
    Addi::new(1, 2, 2048);
}

// ---------------------------------------------------------------------------
// Register name generation
// ---------------------------------------------------------------------------

#[test]
fn reg_name_integer() {
    assert_eq!(reg_name(0, false), "zero");
    assert_eq!(reg_name(1, false), "ra");
    assert_eq!(reg_name(2, false), "sp");
    assert_eq!(reg_name(5, false), "t0");
    assert_eq!(reg_name(8, false), "s0");
    assert_eq!(reg_name(10, false), "a0");
    assert_eq!(reg_name(28, false), "t3");
    assert_eq!(reg_name(31, false), "t6");
    assert_eq!(reg_name(32, false), "x32");
}

#[test]
fn reg_name_fp() {
    assert_eq!(reg_name(0, true), "ft0");
    assert_eq!(reg_name(8, true), "fs0");
    assert_eq!(reg_name(10, true), "fa0");
    assert_eq!(reg_name(28, true), "ft8");
    assert_eq!(reg_name(31, true), "ft11");
    assert_eq!(reg_name(32, true), "f32");
}

// ---------------------------------------------------------------------------
// Decode round‑trip
// ---------------------------------------------------------------------------

#[test]
fn rtype_decode() {
    let word: u32 = 0x003100b3;
    let dec = RType::decode(word);
    assert_eq!(dec.opcode, 0x33);
    assert_eq!(dec.rd, 1);
    assert_eq!(dec.rs1, 2);
    assert_eq!(dec.rs2, 3);
    assert_eq!(dec.funct3, 0);
    assert_eq!(dec.funct7, 0x00);
}

#[test]
fn itype_decode() {
    let word: u32 = 0x02a10093;
    let dec = IType::decode(word);
    assert_eq!(dec.rd, 1);
    assert_eq!(dec.rs1, 2);
    assert_eq!(dec.imm, 42);
}
