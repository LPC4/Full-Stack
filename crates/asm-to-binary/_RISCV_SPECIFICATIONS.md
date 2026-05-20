# RISC-V RV64IMAFD Assembler Specification

**Version:** 1.0.1
**Target Architecture:** RISC-V 64-bit Base Integer (RV64I) + Multiply/Divide (M) + Atomics (A) + Single/Double Precision Floating Point (F/D)  
**Document Purpose:** Complete reference for implementing a RISC-V assembler. All instructions, encodings, ABI rules, and validation constraints for RV64IMAFD are included.

---

## 1. Architecture Overview
- **Base:** RV64I (64-bit integer registers, PC-relative branches/jumps, two's complement arithmetic)
- **Extensions:** M (hardware mul/div), A (atomic memory ops), F (32-bit FP), D (64-bit FP)
- **Zicsr / Zifencei:** CSR instructions (formerly implicit in I) now belong to the `Zicsr` extension; `fence.i` belongs to `Zifencei`. Assemblers targeting general-purpose RV64GC code should support both unconditionally.
- **Instruction Width:** 32-bit, 4-byte aligned (no C-extension compression)
- **Endianness:** Little-endian
- **Memory Model:** RVWMO (Weak Memory Ordering). A-extension provides acquire/release ordering via `aq`/`rl` bits.
- **Privilege:** Machine (M), Supervisor (S), User (U). Assembler targets U/S mode conventions by default.

---

## 2. Register File

### Integer Registers (x0-x31)

| Register | ABI Name | Description | Callee-Saved? |
|----------|----------|-------------|----------------|
| `x0` | `zero` | Hardwired zero | N/A |
| `x1` | `ra` | Return address | No |
| `x2` | `sp` | Stack pointer | **Yes** |
| `x3` | `gp` | Global pointer | **Yes** |
| `x4` | `tp` | Thread pointer | **Yes** |
| `x5`-`x7` | `t0`-`t2` | Temporaries | No |
| `x8` | `s0`/`fp` | Saved / frame pointer | **Yes** |
| `x9` | `s1` | Saved register | **Yes** |
| `x10`-`x11` | `a0`-`a1` | Function args / return values | No |
| `x12`-`x17` | `a2`-`a7` | Function args | No |
| `x18`-`x27` | `s2`-`s11` | Saved registers | **Yes** |
| `x28`-`x31` | `t3`-`t6` | Temporaries | No |

### Floating-Point Registers (f0-f31)

| Register | ABI Name | Description | Callee-Saved? |
|----------|----------|-------------|----------------|
| `f0`-`f7` | `ft0`-`ft7` | FP Temporaries | No |
| `f8`-`f9` | `fs0`-`fs1` | FP Saved | **Yes** |
| `f10`-`f11` | `fa0`-`fa1` | FP Args / return values | No |
| `f12`-`f17` | `fa2`-`fa7` | FP Args | No |
| `f18`-`f27` | `fs2`-`fs11` | FP Saved | **Yes** |
| `f28`-`f31` | `ft8`-`ft11` | FP Temporaries | No |

**Note:** With the D extension, each `f` register is 64 bits wide. Single-precision values are stored NaN-boxed in the lower 32 bits. There is **no** even/odd register restriction; any `f0`-`f31` is valid in any FP instruction.

---

## 3. Instruction Formats (Bit 31 → 0)

```
R-type:  [funct7:31-25][rs2:24-20][rs1:19-15][funct3:14-12][rd:11-7][opcode:6-0]
I-type:  [imm[11:0]:31-20][rs1:19-15][funct3:14-12][rd:11-7][opcode:6-0]
S-type:  [imm[11:5]:31-25][rs2:24-20][rs1:19-15][funct3:14-12][imm[4:0]:11-7][opcode:6-0]
B-type:  [imm[12|10:5]:31-25][rs2:24-20][rs1:19-15][funct3:14-12][imm[4:1|11]:11-7][opcode:6-0]
U-type:  [imm[31:12]:31-12][rd:11-7][opcode:6-0]
J-type:  [imm[20|10:1|11|19:12]:31-12][rd:11-7][opcode:6-0]
R4-type: [rs3:31-27][fmt:26-25][rs2:24-20][rs1:19-15][rm:14-12][rd:11-7][opcode:6-0]
  (R4-type is used exclusively by FMADD/FMSUB/FNMSUB/FNMADD)
```

### RV64 Shift Encoding Note
In RV64I, non-`*W` shift instructions (SLLI/SRLI/SRAI) use a **6-bit shift amount** (`shamt[5:0]`) in bits 25-20. Bits 31-26 form a **funct6** field (`0x00` for SLLI/SRLI, `0x10` for SRAI). The official ISA manual also refers to this as `funct7` with bit 25 being part of the immediate (making `srai` have `funct7=0x20`). Either framing encodes identically; the critical thing is that bit 30 = 1 for SRAI, bits 31 and 29-26 = 0. `*W` shifts use only `shamt[4:0]` (bits 24-20); bit 25 must be 0.

### FP Format Field (`fmt`) - 2-bit, bits [26:25]
```
00 = Single (S)
01 = Double (D)
10 = Half (H) - Zfh extension only
11 = Quad (Q) - Q extension only
```
For RV64IMAFD, only `00` (S) and `01` (D) are used.

### Atomic Instruction Format
```
[funct5:31-27][aq:26][rl:25][rs2:24-20][rs1:19-15][funct3:14-12][rd:11-7][opcode:6-0]
```

---

## 4. Immediate Encoding & Offset Calculations

| Format | Bit Field | Sign-Extended | Usage |
|--------|-----------|---------------|-------|
| I | `imm[11:0]` (bits 31-20) | Yes | `addi`, `jalr`, loads |
| S | `imm[11:5]`\|`imm[4:0]` | Yes | Stores |
| B | `imm[12]`\|`imm[10:5]`\|`imm[4:1]`\|`imm[11]` | Yes, `<<1` | Branches |
| U | `imm[31:12]` (bits 31-12) | No (upper 20) | `lui`, `auipc` |
| J | `imm[20]`\|`imm[10:1]`\|`imm[11]`\|`imm[19:12]` | Yes, `<<1` | `jal` |

### Branch Offset Calculation (B-type)
```
target_pc = current_pc + sext(imm << 1)
```
- `imm[0]` is always implicitly `0` (targets must be 2-byte aligned; 4-byte aligned without C extension)
- `imm[12]` is the sign bit
- **Valid byte offset range: `[-4096, +4094]`** (even values only; ±4096 in multiples of 2 means offset ∈ {-4096, -4094, ..., 0, ..., +4094})
- Assembler must **error** if offset is outside this range or is odd.

### Jump Offset Calculation (J-type)
```
target_pc = current_pc + sext(imm << 1)
```
- **Valid byte offset range: `[-1048576, +1048574]`** (even values only)
- Target must be 4-byte aligned (without C extension).

### `lui` / `auipc` Upper Immediate (U-type)
- The 20-bit `imm[31:12]` is placed in bits 31-12 of the result; bits 11-0 are zero.
- **Important:** If the 12-bit low part of a symbol's address has bit 11 set (value ≥ 0x800), the upper immediate must be incremented by 1 to compensate for sign extension.

---

## 5. Complete Instruction Reference

### 5.1 RV64I Base Integer

#### Load Instructions (I-type, opcode `0x03`)
| Mnemonic | funct3 | Semantics | Notes |
|----------|--------|-----------|-------|
| `lb` | `0` | `rd = sext(M[rs1+imm][7:0])` | |
| `lh` | `1` | `rd = sext(M[rs1+imm][15:0])` | |
| `lw` | `2` | `rd = sext(M[rs1+imm][31:0])` | |
| `ld` | `3` | `rd = M[rs1+imm][63:0]` | RV64 only |
| `lbu` | `4` | `rd = zext(M[rs1+imm][7:0])` | |
| `lhu` | `5` | `rd = zext(M[rs1+imm][15:0])` | |
| `lwu` | `6` | `rd = zext(M[rs1+imm][31:0])` | RV64 only |

#### Store Instructions (S-type, opcode `0x23`)
| Mnemonic | funct3 | Semantics |
|----------|--------|-----------|
| `sb` | `0` | `M[rs1+imm][7:0] = rs2[7:0]` |
| `sh` | `1` | `M[rs1+imm][15:0] = rs2[15:0]` |
| `sw` | `2` | `M[rs1+imm][31:0] = rs2[31:0]` |
| `sd` | `3` | `M[rs1+imm][63:0] = rs2[63:0]` |

#### Integer Register-Immediate (I-type, opcode `0x13`)
| Mnemonic | funct3 | funct7/funct6 | Semantics | Notes |
|----------|--------|---------------|-----------|-------|
| `addi` | `0` | - | `rd = rs1 + sext(imm)` | `addi rd, x0, 0` = canonical NOP |
| `slti` | `2` | - | `rd = (rs1 <ₛ sext(imm)) ? 1 : 0` | |
| `sltiu` | `3` | - | `rd = (rs1 <ᵤ sext(imm)) ? 1 : 0` | imm is sign-extended then compared unsigned |
| `xori` | `4` | - | `rd = rs1 ^ sext(imm)` | `xori rd, rs1, -1` = bitwise NOT |
| `ori` | `6` | - | `rd = rs1 \| sext(imm)` | |
| `andi` | `7` | - | `rd = rs1 & sext(imm)` | |
| `slli` | `1` | `funct6=0x00` (bits[31:26]=000000) | `rd = rs1 << imm[5:0]` | shamt is 6-bit |
| `srli` | `5` | `funct6=0x00` (bits[31:26]=000000) | `rd = rs1 >>ᵤ imm[5:0]` | shamt is 6-bit |
| `srai` | `5` | `funct6=0x10` (bit30=1, bits[31,29:26]=0) | `rd = rs1 >>ₛ imm[5:0]` | shamt is 6-bit |

#### Integer Register-Register (R-type, opcode `0x33`)
| Mnemonic | funct3 | funct7 | Semantics |
|----------|--------|--------|-----------|
| `add` | `0` | `0x00` | `rd = rs1 + rs2` |
| `sub` | `0` | `0x20` | `rd = rs1 - rs2` |
| `sll` | `1` | `0x00` | `rd = rs1 << rs2[5:0]` |
| `slt` | `2` | `0x00` | `rd = (rs1 <ₛ rs2) ? 1 : 0` |
| `sltu` | `3` | `0x00` | `rd = (rs1 <ᵤ rs2) ? 1 : 0` |
| `xor` | `4` | `0x00` | `rd = rs1 ^ rs2` |
| `srl` | `5` | `0x00` | `rd = rs1 >>ᵤ rs2[5:0]` |
| `sra` | `5` | `0x20` | `rd = rs1 >>ₛ rs2[5:0]` |
| `or` | `6` | `0x00` | `rd = rs1 \| rs2` |
| `and` | `7` | `0x00` | `rd = rs1 & rs2` |

#### Control Transfer
| Mnemonic | Type | Opcode | funct3 | Semantics | Notes |
|----------|------|--------|--------|-----------|-------|
| `jal` | J | `0x6F` | - | `rd = pc+4; pc += sext(imm<<1)` | offset range ±1MiB (even) |
| `jalr` | I | `0x67` | `0` | `rd = pc+4; pc = (rs1+sext(imm)) & ~1` | clears bit 0 |
| `beq` | B | `0x63` | `0` | `if rs1==rs2: pc += sext(imm<<1)` | |
| `bne` | B | `0x63` | `1` | `if rs1!=rs2: pc += sext(imm<<1)` | |
| `blt` | B | `0x63` | `4` | `if rs1<ₛrs2: pc += sext(imm<<1)` | |
| `bge` | B | `0x63` | `5` | `if rs1>=ₛrs2: pc += sext(imm<<1)` | |
| `bltu` | B | `0x63` | `6` | `if rs1<ᵤrs2: pc += sext(imm<<1)` | |
| `bgeu` | B | `0x63` | `7` | `if rs1>=ᵤrs2: pc += sext(imm<<1)` | |

#### Upper Immediate
| Mnemonic | Type | Opcode | Semantics |
|----------|------|--------|-----------|
| `lui` | U | `0x37` | `rd = imm[31:12] << 12` (lower 12 bits zero) |
| `auipc` | U | `0x17` | `rd = pc + (imm[31:12] << 12)` |

#### System / Fence (Zicsr, Zifencei)
| Mnemonic | Type | Opcode | funct3 | Semantics | Notes |
|----------|------|--------|--------|-----------|-------|
| `fence` | I | `0x0F` | `0` | Memory ordering fence; `imm[11:8]`=`fm`, `imm[7:4]`=`pred`, `imm[3:0]`=`succ` | Zifencei |
| `fence.i` | I | `0x0F` | `1` | I-cache synchronize; `imm=0`, `rs1=0`, `rd=0` | Zifencei |
| `ecall` | I | `0x73` | `0` | Environment call trap; `imm=0x000`, all other fields = 0 | |
| `ebreak` | I | `0x73` | `0` | Breakpoint trap; `imm=0x001`, all other fields = 0 | |

#### RV64I Word (W) Instructions
*Operate on lower 32 bits; result sign-extended to 64 bits.*

| Mnemonic | Type | Opcode | funct3 | funct7 | Semantics |
|----------|------|--------|--------|--------|-----------|
| `addiw` | I | `0x1B` | `0` | - | `rd = sext32(rs1[31:0] + sext(imm))` |
| `slliw` | I | `0x1B` | `1` | `0x00` | `rd = sext32(rs1[31:0] << shamt[4:0])`; shamt[5] must be 0 |
| `srliw` | I | `0x1B` | `5` | `0x00` | `rd = sext32(rs1[31:0] >>ᵤ shamt[4:0])`; shamt[5] must be 0 |
| `sraiw` | I | `0x1B` | `5` | `0x20` | `rd = sext32(rs1[31:0] >>ₛ shamt[4:0])`; shamt[5] must be 0 |
| `addw` | R | `0x3B` | `0` | `0x00` | `rd = sext32(rs1[31:0] + rs2[31:0])` |
| `subw` | R | `0x3B` | `0` | `0x20` | `rd = sext32(rs1[31:0] - rs2[31:0])` |
| `sllw` | R | `0x3B` | `1` | `0x00` | `rd = sext32(rs1[31:0] << rs2[4:0])` |
| `srlw` | R | `0x3B` | `5` | `0x00` | `rd = sext32(rs1[31:0] >>ᵤ rs2[4:0])` |
| `sraw` | R | `0x3B` | `5` | `0x20` | `rd = sext32(rs1[31:0] >>ₛ rs2[4:0])` |

---

### 5.2 RV64M - Multiply/Divide Extension

All R-type. Non-W instructions: opcode `0x33`, `funct7=0x01`. W instructions: opcode `0x3B`, `funct7=0x01`.

#### 64-bit Operations (opcode `0x33`)
| Mnemonic | funct3 | Semantics | Notes |
|----------|--------|-----------|-------|
| `mul` | `0` | `rd = (rs1 × rs2)[63:0]` | Low 64 bits of full product |
| `mulh` | `1` | `rd = (rs1 ×ₛₛ rs2)[127:64]` | Signed × signed, upper 64 bits |
| `mulhsu` | `2` | `rd = (rs1 ×ₛᵤ rs2)[127:64]` | Signed × unsigned, upper 64 bits |
| `mulhu` | `3` | `rd = (rs1 ×ᵤᵤ rs2)[127:64]` | Unsigned × unsigned, upper 64 bits |
| `div` | `4` | `rd = rs1 /ₛ rs2` | div/0 → −1; overflow (INT64_MIN/−1) → INT64_MIN |
| `divu` | `5` | `rd = rs1 /ᵤ rs2` | div/0 → 2⁶⁴−1 |
| `rem` | `6` | `rd = rs1 %ₛ rs2` | div/0 → rs1; overflow → 0 |
| `remu` | `7` | `rd = rs1 %ᵤ rs2` | div/0 → rs1 |

#### 32-bit Word Operations (opcode `0x3B`)
*Operate on lower 32 bits; result sign-extended to 64 bits.*

| Mnemonic | funct3 | Semantics |
|----------|--------|-----------|
| `mulw` | `0` | `rd = sext32((rs1[31:0] × rs2[31:0])[31:0])` |
| `divw` | `4` | `rd = sext32(rs1[31:0] /ₛ rs2[31:0])` |
| `divuw` | `5` | `rd = sext32(rs1[31:0] /ᵤ rs2[31:0])` |
| `remw` | `6` | `rd = sext32(rs1[31:0] %ₛ rs2[31:0])` |
| `remuw` | `7` | `rd = sext32(rs1[31:0] %ᵤ rs2[31:0])` |

---

### 5.3 RV64A - Atomics Extension

**Encoding:** R-type variant, opcode `0x2F`. `funct3` selects width: `2`=word (`.w`), `3`=doubleword (`.d`). **`funct5`** (bits 31-27) selects the operation. **`aq`** (bit 26) and **`rl`** (bit 25) control acquire/release ordering.

```
[funct5:31-27][aq:26][rl:25][rs2:24-20][rs1:19-15][funct3:14-12][rd:11-7][0101111]
```

#### Correct funct5 Table
| Mnemonic | funct5 (binary) | funct5 (hex) | Semantics |
|----------|----------------|-------------|-----------|
| `amoadd.w/d` | `00000` | `0x00` | `t=M[rs1]; rd=t; M[rs1]=t+rs2` |
| `amoswap.w/d` | `00001` | `0x01` | `t=M[rs1]; rd=t; M[rs1]=rs2` |
| `lr.w/d` | `00010` | `0x02` | `rd=M[rs1]; register reservation on rs1. rs2 must be x0.` |
| `sc.w/d` | `00011` | `0x03` | `if reserved: M[rs1]=rs2; rd=0; else rd=1. rd=x0 is valid.` |
| `amoxor.w/d` | `00100` | `0x04` | `t=M[rs1]; rd=t; M[rs1]=t^rs2` |
| `amoand.w/d` | `01100` | `0x0C` | `t=M[rs1]; rd=t; M[rs1]=t&rs2` |
| `amoor.w/d` | `01000` | `0x08` | `t=M[rs1]; rd=t; M[rs1]=t\|rs2` |
| `amomin.w/d` | `10000` | `0x10` | `t=M[rs1]; rd=t; M[rs1]=min(t, rs2)` (signed) |
| `amomax.w/d` | `10100` | `0x14` | `t=M[rs1]; rd=t; M[rs1]=max(t, rs2)` (signed) |
| `amominu.w/d` | `11000` | `0x18` | `t=M[rs1]; rd=t; M[rs1]=min(t, rs2)` (unsigned) |
| `amomaxu.w/d` | `11100` | `0x1C` | `t=M[rs1]; rd=t; M[rs1]=max(t, rs2)` (unsigned) |

**Critical notes:**
-  AMO semantics: `rs1` holds the **address**. The loaded value is captured into a temp, returned in `rd`, and the result of the operation is stored back. `rs1` itself is never used as an arithmetic operand.
- `sc` with `rd=x0` is valid; success/failure status is discarded.
- `aq`/`rl` default to `0`. Assembler syntax supports `.aq`, `.rl`, `.aqrl` suffixes.
- `lr`/`sc` addresses must be naturally aligned: 4 bytes for `.w`, 8 bytes for `.d`.

---

### 5.4 RV64F/D - Floating-Point Extensions

**Opcode summary:**
- Loads: `0x07` (FP load)
- Stores: `0x27` (FP store)
- ALU / Convert / Compare / Classify: `0x53` (OP-FP)
- FMAC: `0x43` (FMADD), `0x47` (FMSUB), `0x4B` (FNMSUB), `0x4F` (FNMADD)

**`fmt` field (bits 26:25):** `00`=Single (`.s`), `01`=Double (`.d`). This is a **2-bit field**, not 1 bit.

**Rounding mode (`rm`, bits 14-12):** `000`=RNE, `001`=RTZ, `010`=RDN, `011`=RUP, `100`=RMME, `111`=DYN (from `fcsr.frm`)

#### FP Loads/Stores (I-type / S-type)
| Mnemonic | Opcode | funct3 | Semantics |
|----------|--------|--------|-----------|
| `flw` | `0x07` | `2` | `fd = NaN-box(M[rs1+imm][31:0])` |
| `fld` | `0x07` | `3` | `fd = M[rs1+imm][63:0]` |
| `fsw` | `0x27` | `2` | `M[rs1+imm][31:0] = fs2[31:0]` |
| `fsd` | `0x27` | `3` | `M[rs1+imm][63:0] = fs2[63:0]` |

#### FP ALU (R-type, opcode `0x53`)

All FP ALU ops: `funct5` selects operation, `fmt` selects precision.

| Mnemonic | funct5 (bin) | rm | Semantics | Notes |
|----------|--------------|----|-----------|-------|
| `fadd.s/d` | `00000` | dynamic | `fd = fs1 + fs2` | |
| `fsub.s/d` | `00001` | dynamic | `fd = fs1 - fs2` | |
| `fmul.s/d` | `00010` | dynamic | `fd = fs1 * fs2` | |
| `fdiv.s/d` | `00011` | dynamic | `fd = fs1 / fs2` | |
| `fsqrt.s/d` | `01011` | dynamic | `fd = √fs1`; `rs2=00000` | |
| `fsgnj.s/d` | `00100` | `000` | `fd = {fs2.sign, fs1.exp, fs1.mantissa}` | sign from rs2 |
| `fsgnjn.s/d` | `00100` | `001` | `fd = {~fs2.sign, fs1.exp, fs1.mantissa}` | inverted sign from rs2 |
| `fsgnjx.s/d` | `00100` | `010` | `fd = {fs1.sign^fs2.sign, fs1.exp, fs1.mantissa}` | XOR sign |
| `fmin.s/d` | `00101` | `000` | `fd = min(fs1, fs2)` | −0 < +0; NaN handling per IEEE |
| `fmax.s/d` | `00101` | `001` | `fd = max(fs1, fs2)` | |

**Note:** `fsgnj`, `fsgnjn`, `fsgnjx` share **the same `funct5=00100`**, distinguished by `rm`. Likewise `fmin`/`fmax` share `funct5=00101`, distinguished by `rm=000`/`001`.

#### FP Move Between FP and Integer Registers (R-type, opcode `0x53`)
These instructions transfer bit patterns without conversion. `rs2` must be `00000`.

| Mnemonic | funct5 | fmt | rm | Semantics |
|----------|--------|-----|----|-----------|
| `fmv.x.w` | `11100` | `00` | `000` | `rd = sext32(fs1[31:0])` - FP→int, single bits |
| `fmv.w.x` | `11110` | `00` | `000` | `fd = NaN-box(rs1[31:0])` - int→FP, single bits |
| `fmv.x.d` | `11100` | `01` | `000` | `rd = fs1[63:0]` - FP→int, double bits (RV64 only) |
| `fmv.d.x` | `11110` | `01` | `000` | `fd = rs1[63:0]` - int→FP, double bits (RV64 only) |

#### FP Compare and Classify (R-type, opcode `0x53`)
| Mnemonic | funct5 | rm/funct3 | Semantics | Notes |
|----------|--------|-----------|-----------|-------|
| `feq.s/d` | `10100` | `010` | `rd = (fs1 == fs2) ? 1 : 0` | Quiet NaN → 0, sets no flag |
| `flt.s/d` | `10100` | `001` | `rd = (fs1 < fs2) ? 1 : 0` | sNaN → invalid flag |
| `fle.s/d` | `10100` | `000` | `rd = (fs1 <= fs2) ? 1 : 0` | sNaN → invalid flag |
| `fclass.s/d` | `11100` | `001` | `rd = classify(fs1)` (bitmask) | **`rs2` must be `x0`/`f0`; assembler must error otherwise** |

`fclass` result bitmask (bits 9-0):
- bit 0: −∞, bit 1: negative normal, bit 2: negative subnormal, bit 3: −0
- bit 4: +0, bit 5: positive subnormal, bit 6: positive normal, bit 7: +∞
- bit 8: signaling NaN, bit 9: quiet NaN

#### FP Conversion Instructions (R-type, opcode `0x53`)

**Key principle:** `funct5` encodes the conversion **direction** only. The `rs2` field (bits 24-20) encodes the **integer type** for FP↔int conversions. For FP↔FP conversions, `rs2` encodes the source format.

##### Integer type codes (rs2 field for FP↔int)
| rs2 value | Integer type |
|-----------|-------------|
| `00000` | `w` - 32-bit signed |
| `00001` | `wu` - 32-bit unsigned |
| `00010` | `l` - 64-bit signed (RV64 only) |
| `00011` | `lu` - 64-bit unsigned (RV64 only) |

##### FP → Integer Conversions (`funct5 = 11000`, result in integer `rd`)
`fmt` selects the source FP type; `rs2` selects the integer destination type.

| Mnemonic | fmt | rs2 | Semantics |
|----------|-----|-----|-----------|
| `fcvt.w.s` | `00` | `00000` | `rd = (int32_t)fs1` |
| `fcvt.wu.s` | `00` | `00001` | `rd = (uint32_t)fs1` |
| `fcvt.l.s` | `00` | `00010` | `rd = (int64_t)fs1` (RV64 only) |
| `fcvt.lu.s` | `00` | `00011` | `rd = (uint64_t)fs1` (RV64 only) |
| `fcvt.w.d` | `01` | `00000` | `rd = (int32_t)fs1` |
| `fcvt.wu.d` | `01` | `00001` | `rd = (uint32_t)fs1` |
| `fcvt.l.d` | `01` | `00010` | `rd = (int64_t)fs1` (RV64 only) |
| `fcvt.lu.d` | `01` | `00011` | `rd = (uint64_t)fs1` (RV64 only) |

##### Integer → FP Conversions (`funct5 = 11010`, result in FP `fd`)
`fmt` selects the destination FP type; `rs2` selects the integer source type.

| Mnemonic | fmt | rs2 | Semantics |
|----------|-----|-----|-----------|
| `fcvt.s.w` | `00` | `00000` | `fd = (float)rs1` (from int32) |
| `fcvt.s.wu` | `00` | `00001` | `fd = (float)rs1` (from uint32) |
| `fcvt.s.l` | `00` | `00010` | `fd = (float)rs1` (from int64, RV64 only) |
| `fcvt.s.lu` | `00` | `00011` | `fd = (float)rs1` (from uint64, RV64 only) |
| `fcvt.d.w` | `01` | `00000` | `fd = (double)rs1` (from int32) |
| `fcvt.d.wu` | `01` | `00001` | `fd = (double)rs1` (from uint32) |
| `fcvt.d.l` | `01` | `00010` | `fd = (double)rs1` (from int64, RV64 only) |
| `fcvt.d.lu` | `01` | `00011` | `fd = (double)rs1` (from uint64, RV64 only) |

##### FP ↔ FP Conversions (opcode `0x53`)
The `fmt` field encodes the **destination** format; `rs2` encodes the **source** format.

| Mnemonic | funct5 | fmt | rs2 | Semantics |
|----------|--------|-----|-----|-----------|
| `fcvt.s.d` | `01000` | `00` (S dest) | `00001` (D src) | `fd = (float)fs1` (double→single, may round) |
| `fcvt.d.s` | `01001` | `01` (D dest) | `00000` (S src) | `fd = (double)fs1` (single→double, exact) |

**Validation:** Assembler must reject `fcvt.<T>.<T>` where source and destination types are identical (e.g., `fcvt.s.s`, `fcvt.w.w`).

#### FMAC Instructions (R4-type)
`fd = ±(fs1 × fs2) ± fs3`
Format: `[rs3:31-27][fmt:26-25][rs2:24-20][rs1:19-15][rm:14-12][rd:11-7][opcode:6-0]`

| Mnemonic | opcode | Semantics |
|----------|--------|-----------|
| `fmadd.s/d` | `0x43` | `fd = (fs1 × fs2) + fs3` |
| `fmsub.s/d` | `0x47` | `fd = (fs1 × fs2) − fs3` |
| `fnmsub.s/d` | `0x4B` | `fd = −(fs1 × fs2) + fs3` |
| `fnmadd.s/d` | `0x4F` | `fd = −(fs1 × fs2) − fs3` |

---

## 6. CSR Instructions (Zicsr Extension)
I-type, opcode `0x73`. `csr` is the 12-bit CSR address (bits 31-20). For immediate variants (`csrrwi`, `csrrsi`, `csrrci`), the 5-bit zero-extended unsigned immediate (`uimm`) is in the `rs1` field (bits 19-15).

| Mnemonic | funct3 | Semantics |
|----------|--------|-----------|
| `csrrw` | `1` | `rd = CSR; CSR = rs1`. If `rd=x0`, CSR is not read (no side effects from read). |
| `csrrs` | `2` | `rd = CSR; CSR \|= rs1`. If `rs1=x0`, CSR is not written. |
| `csrrc` | `3` | `rd = CSR; CSR &= ~rs1`. If `rs1=x0`, CSR is not written. |
| `csrrwi` | `5` | `rd = CSR; CSR = zext(uimm[4:0])` |
| `csrrsi` | `6` | `rd = CSR; CSR \|= zext(uimm[4:0])`. If `uimm=0`, CSR not written. |
| `csrrci` | `7` | `rd = CSR; CSR &= ~zext(uimm[4:0])`. If `uimm=0`, CSR not written. |

### Common CSR Addresses
| CSR Name | Address | Description |
|----------|---------|-------------|
| `fflags` | `0x001` | FP accrued exception flags |
| `frm` | `0x002` | FP dynamic rounding mode |
| `fcsr` | `0x003` | FP control and status (fflags + frm) |
| `cycle` | `0xC00` | Cycle counter (read-only, user) |
| `time` | `0xC01` | Timer (read-only, user) |
| `instret` | `0xC02` | Instructions retired (read-only, user) |
| `mstatus` | `0x300` | Machine status (includes FP enable `fs` field) |
| `misa` | `0x301` | ISA and extensions |
| `mtvec` | `0x305` | Trap vector base address |
| `mepc` | `0x341` | Machine exception program counter |
| `mcause` | `0x342` | Machine trap cause |
| `mtval` | `0x343` | Machine bad address or instruction |

---

## 7. Standard Pseudo-Instructions

| Pseudo | Canonical Expansion | Notes |
|--------|---------------------|-------|
| `nop` | `addi x0, x0, 0` | No operation |
| `li rd, imm` | `addi rd, x0, imm` (if fits 12-bit)<br>`lui rd, imm[31:12]; addi rd, rd, imm[11:0]` (32-bit)<br>multi-instruction for 64-bit values | Assembler selects best encoding |
| `la rd, symbol` | `auipc rd, %pcrel_hi(symbol); addi rd, rd, %pcrel_lo(symbol)(rd)` | PC-relative address load |
| `mv rd, rs` | `addi rd, rs, 0` | Register copy |
| `not rd, rs` | `xori rd, rs, -1` | Bitwise NOT |
| `neg rd, rs` | `sub rd, x0, rs` | Two's complement negation |
| `negw rd, rs` | `subw rd, x0, rs` | 32-bit negation, sign-extend |
| `sext.w rd, rs` | `addiw rd, rs, 0` | Sign-extend 32→64 |
| `seqz rd, rs` | `sltiu rd, rs, 1` | Set if equal to zero |
| `snez rd, rs` | `sltu rd, x0, rs` | Set if not equal to zero |
| `sltz rd, rs` | `slt rd, rs, x0` | Set if less than zero |
| `sgtz rd, rs` | `slt rd, x0, rs` | Set if greater than zero |
| `beqz rs, L` | `beq rs, x0, L` | Branch if equal to zero |
| `bnez rs, L` | `bne rs, x0, L` | Branch if not equal to zero |
| `blez rs, L` | `bge x0, rs, L` | Branch if less than or equal to zero |
| `bgez rs, L` | `bge rs, x0, L` | Branch if greater than or equal to zero |
| `bltz rs, L` | `blt rs, x0, L` | Branch if less than zero |
| `bgtz rs, L` | `blt x0, rs, L` | Branch if greater than zero |
| `bgt rs1, rs2, L` | `blt rs2, rs1, L` | Branch if rs1 > rs2 (signed) |
| `ble rs1, rs2, L` | `bge rs2, rs1, L` | Branch if rs1 ≤ rs2 (signed) |
| `bgtu rs1, rs2, L` | `bltu rs2, rs1, L` | Branch if rs1 > rs2 (unsigned) |
| `bleu rs1, rs2, L` | `bgeu rs2, rs1, L` | Branch if rs1 ≤ rs2 (unsigned) |
| `j L` | `jal x0, L` | Unconditional jump (no link) |
| `jr rs` | `jalr x0, 0(rs)` | Jump to register |
| `ret` | `jalr x0, 0(ra)` | Return from function |
| `call L` | `auipc ra, %pcrel_hi(L); jalr ra, %pcrel_lo(L)(ra)` | Call far function |
| `tail L` | `auipc t1, %pcrel_hi(L); jalr x0, %pcrel_lo(L)(t1)` | Tail call (no link) |
| `fence` | `fence iorw, iorw` | Full memory fence shorthand |

#### FP Pseudo-Instructions
| Pseudo | Expansion | Notes |
|--------|-----------|-------|
| `fmv.s fd, fs` | `fsgnj.s fd, fs, fs` | FP register copy (single) |
| `fmv.d fd, fs` | `fsgnj.d fd, fs, fs` | FP register copy (double) |
| `fneg.s fd, fs` | `fsgnjn.s fd, fs, fs` | Negate single (flip sign) |
| `fneg.d fd, fs` | `fsgnjn.d fd, fs, fs` | Negate double |
| `fabs.s fd, fs` | `fsgnjx.s fd, fs, fs` | Absolute value single |
| `fabs.d fd, fs` | `fsgnjx.d fd, fs, fs` | Absolute value double |

**Removed pseudo:** `fmvp.s/d` - this is **not** a standard RISC-V pseudo-instruction and must not be used. Use `fmv.w.x` / `fmv.d.x` to move from integer to FP, or `fmv.s` / `fmv.d` to copy between FP registers.

---

## 8. RV64IMAFD ABI & Calling Convention

### Argument Passing
- **Integer / Pointer:** `a0`-`a7` (`x10`-`x17`)
- **Floating Point:** `fa0`-`fa7` (`f10`-`f17`)
- **Small structs/aggregates (≤ 16 bytes):** May be passed in up to two registers (integer or FP as applicable)
- **Large structs (> 16 bytes):** Passed by reference (pointer in integer register)
- **Variadic / `va_list` arguments:** Per the RISC-V C ABI, all floating-point arguments that fall within the variable portion of a variadic argument list must be passed in **integer registers** (or on the stack), not FP registers. This applies regardless of available FP register slots.

### Return Values
- **Integer / Pointer:** `a0`, `a1` (`x10`, `x11`)
- **FP Single/Double:** `fa0`, `fa1` (`f10`, `f11`)

### Stack Frame
- **Alignment:** 16-byte aligned at all call boundaries (sp must be ≡ 0 mod 16 on function entry)
- **Direction:** Grows downward
- **Layout (high → low):** saved registers → local variables → outgoing argument overflow area (if >8 int args or >8 FP args)

### Callee-Saved Registers
- Integer: `sp`, `gp`, `tp`, `s0`-`s11` (`x2`-`x4`, `x8`-`x9`, `x18`-`x27`)
- FP: `fs0`-`fs11` (`f8`-`f9`, `f18`-`f27`)
- All other registers are caller-saved (may be clobbered by callees).

---

## 9. Assembler Implementation Requirements

### 9.1 Parsing & Encoding

1. **Register Names:** Accept both `xN` (0-31) and ABI names (`ra`, `sp`, `a0`, `ft0`, `fa0`, etc.). Reject register numbers > 31. ABI names for FP registers use the same `f0`-`f31` namespace.

2. **Two-Pass Assembly:**
    - **Pass 1:** Scan all labels; record symbol → address mapping. Record section sizes.
    - **Pass 2:** Resolve label references, compute PC-relative offsets, emit machine code and relocations.

3. **Immediate Validation:**
    - I-type / S-type: signed 12-bit: `[-2048, +2047]`
    - B-type: signed 13-bit even: `[-4096, +4094]` ← *(note: upper bound is +4094, not +4096)*
    - J-type: signed 21-bit even: `[-1048576, +1048574]` ← *(note: upper bound is +1048574)*
    - U-type: any value where bits 31-12 fit (upper 20 bits)
    - Non-`*W` shift amounts (`slli`, `srli`, `srai`): `[0, 63]` (6-bit); error on values > 63
    - `*W` shift amounts (`slliw`, `srliw`, `sraiw`, `sllw`, `srlw`, `sraw`): `[0, 31]` (5-bit); error if bit 5 is set; **do not silently mask**
    - CSR addresses: `[0x000, 0xFFF]`
    - `sltiu`: The 12-bit immediate is sign-extended to 64 bits before unsigned comparison; document this explicitly in error messages.

4. **FP `fmt` Field:** `fmt` is a **2-bit field** at bits [26:25]: `00`=S, `01`=D. Assembler must set both bits; masking against bit 25 only is incorrect.

5. **FP Conversion `rs2` field:** For `fcvt.*` instructions, the assembler must explicitly encode the integer type into the `rs2` field (bits 24-20) as: `w=00000`, `wu=00001`, `l=00010`, `lu=00011`. The mnemonic suffix determines this value; `rs2` is **not** a register operand in syntax.

6. **Atomic `aq`/`rl`:** Default both to `0`. Accept `.aq`, `.rl`, `.aqrl` suffixes on AMO/LR/SC mnemonics to set bits 26/25.

7. **`fclass` enforcement:** The `rs2` field of `fclass.s/d` must be `00000`. If the assembler syntax accepts a second operand, it must hard-error if a non-zero value is provided.

8. **`lr.w/d` rs2 field:** `lr` has no second source; the `rs2` field must be `00000`. Assembler should reject any attempt to supply rs2.

### 9.2 Directive Support (Required for Complete Assembler)

| Directive | Purpose |
|-----------|---------|
| `.text` | Switch to text (code) section |
| `.data` | Switch to data section |
| `.bss` | Switch to BSS (zero-initialized) section |
| `.rodata` | Switch to read-only data section |
| `.align n` | Align to 2ⁿ byte boundary |
| `.balign n` | Align to n byte boundary |
| `.p2align n` | Align to 2ⁿ boundary (GAS synonym) |
| `.byte v` | Emit 8-bit value |
| `.half v` / `.2byte v` | Emit 16-bit value |
| `.word v` / `.4byte v` | Emit 32-bit value |
| `.dword v` / `.8byte v` | Emit 64-bit value |
| `.float f` | Emit IEEE 754 single-precision value |
| `.double f` | Emit IEEE 754 double-precision value |
| `.string s` / `.asciz s` | Emit null-terminated ASCII string |
| `.ascii s` | Emit ASCII string (no null terminator) |
| `.globl sym` | Mark symbol as globally visible |
| `.local sym` | Mark symbol as local |
| `.weak sym` | Mark symbol as weak |
| `.type sym, @function` / `@object` | Set ELF symbol type |
| `.size sym, expr` | Set ELF symbol size |
| `.equ sym, val` | Define assembler constant |
| `.set sym, val` | Synonym for `.equ` |
| `.skip n` / `.space n` | Emit n zero bytes |
| `.zero n` | Emit n zero bytes |

### 9.3 Relocations (ELF `R_RISCV_*`)

| Relocation | Format | Usage |
|------------|--------|-------|
| `R_RISCV_32` | U32 | Absolute 32-bit symbol reference |
| `R_RISCV_64` | U64 | Absolute 64-bit symbol reference |
| `R_RISCV_HI20` | U-type[31:12] | `%hi(symbol)` - upper 20 bits |
| `R_RISCV_LO12_I` | I-type[11:0] | `%lo(symbol)` - lower 12 bits, load/imm |
| `R_RISCV_LO12_S` | S-type split | `%lo(symbol)` - lower 12 bits, store |
| `R_RISCV_PCREL_HI20` | U-type[31:12] | `%pcrel_hi(symbol)` for `auipc` |
| `R_RISCV_PCREL_LO12_I` | I-type[11:0] | `%pcrel_lo(label)` - must reference `auipc` label |
| `R_RISCV_PCREL_LO12_S` | S-type split | `%pcrel_lo(label)` - store variant |
| `R_RISCV_BRANCH` | B-type | Branch instructions |
| `R_RISCV_JAL` | J-type | `jal` instruction |
| `R_RISCV_CALL` | U+I pair | `call` pseudo (auipc+jalr pair) |
| `R_RISCV_CALL_PLT` | U+I pair | `call` via PLT |
| `R_RISCV_GOT_HI20` | U-type | GOT-relative `%got_pcrel_hi` |
| `R_RISCV_TLS_GD_HI20` | U-type | TLS GD model |
| `R_RISCV_TLS_GOT_HI20` | U-type | TLS IE model |
| `R_RISCV_TPREL_HI20` | U-type | Thread-pointer-relative, upper |
| `R_RISCV_TPREL_LO12_I` | I-type | Thread-pointer-relative, lower, load |
| `R_RISCV_TPREL_LO12_S` | S-type | Thread-pointer-relative, lower, store |

**`%pcrel_lo` note:** The relocation for `%pcrel_lo` references the **label** of the paired `auipc`, not the register. E.g.:
```asm
.Lpcrel_hi0:
    auipc  a0, %pcrel_hi(target)
    addi   a0, a0, %pcrel_lo(.Lpcrel_hi0)   # references the auipc label
```

---

## 10. Validation & Alignment Rules

- **Instruction alignment:** Without the C extension, all instruction addresses and all branch/jump targets must be **4-byte aligned**. A target at a 2-byte-but-not-4-byte boundary raises `Instruction Address Misaligned`.
- **Branch offset bounds:** B-type byte offsets must be in `[-4096, +4094]` and even. Error on odd or out-of-range values.
- **Jump offset bounds:** J-type byte offsets must be in `[-1048576, +1048574]` and even.
- **Shift amounts:** Non-`*W` shifts accept `[0, 63]`; `*W` shifts accept `[0, 31]`. Assembler must **error**, not silently mask, on oversized values.
- **`sc` with `rd=x0`:** Valid. Hardware attempts the store; success/failure status is discarded.
- **`lr` `rs2` field:** Must be `00000`. Error if assembler source supplies a second register.
- **`fclass` `rs2` field:** Must be `00000`. Error if a second argument is supplied.
- **`fcvt` type validity:** `fcvt.<dst>.<src>` where source and destination FP types are identical is illegal (e.g., `fcvt.s.s`). The assembler must reject this.
- **FP instructions:** Require `mstatus.fs ≠ 0` (FP enabled) at runtime. The assembler does not enforce this but may warn.
- **CSR addresses:** Must be in `[0x000, 0xFFF]`.
- **`sltiu` sign extension:** The assembler must sign-extend the 12-bit immediate to 64 bits for validation purposes (not zero-extend), because the hardware does so before the unsigned comparison.
- **Atomic alignment:** `lr.w`/`sc.w` require 4-byte-aligned address; `lr.d`/`sc.d` require 8-byte-aligned address. Assembler cannot enforce this statically in general but may warn on obvious violations.
- **`fence.i` encoding:** All other fields must be zero: `rs1=0`, `rd=0`, `imm=0`.

---

## Appendix A: Quick Encoding Reference

```
Opcodes (hex):
  LOAD=0x03  LOAD-FP=0x07  MISC-MEM=0x0F  OP-IMM=0x13  AUIPC=0x17
  OP-IMM-32=0x1B  STORE=0x23  STORE-FP=0x27  AMO=0x2F  OP=0x33  LUI=0x37
  OP-32=0x3B  MADD=0x43  MSUB=0x47  NMSUB=0x4B  NMADD=0x4F  OP-FP=0x53
  BRANCH=0x63  JALR=0x67  JAL=0x6F  SYSTEM=0x73

fmt field (bits 26:25):  00=S  01=D  10=H  11=Q

Atomic funct5:
  amoadd=0x00  amoswap=0x01  lr=0x02  sc=0x03
  amoxor=0x04  amoand=0x0C  amoor=0x08
  amomin=0x10  amomax=0x14  amominu=0x18  amomaxu=0x1C

fcvt rs2 field (integer type):
  w=00000  wu=00001  l=00010  lu=00011

Branch range:  [-4096, +4094] bytes (even)
Jump range:    [-1048576, +1048574] bytes (even)
I-imm range:  [-2048, +2047]
Shift (64b):  [0, 63]   Shift (32b/W): [0, 31]

Stack: 16-byte aligned, grows down
ABI args: a0-a7 (int), fa0-fa7 (fp)
ABI ret:  a0-a1 (int), fa0-fa1 (fp)
Varargs FP: passed in integer regs / stack
```

---

## Appendix B: Instruction Encoding Gotchas

1. **`lr` / `sc` are NOT `amoor`:** `lr.w` funct5=`00010`, `sc.w` funct5=`00011`. The previous revision had `lr`=`01000` (same as `amoor`) - a critical bug.

2. **`fmt` is 2 bits:** Setting only bit 25 and leaving bit 26 unset produces incorrect encoding for double-precision. Always set bits [26:25] together: `00` for S, `01` for D.

3. **`fcvt` `rs2` is not an architectural register operand:** The mnemonic suffix determines `rs2`; no register is named in syntax for this field.

4. **`fsgnjn`/`fsgnjx` vs `fsgnj`:** These three share `funct5=00100`. They are distinguished by `rm`: `000`/`001`/`010`. Incorrect encoding of a separate funct5 would produce garbage instructions.

5. **B-type maximum offset is +4094, not +4096:** The encoding cannot represent +4096 (bit 12 would be 1, but that is the sign bit giving −4096). The effective range is −4096 to +4094.

6. **`%pcrel_lo` references the `auipc` label, not the register:** Assembler must emit `R_RISCV_PCREL_LO12_I` pointing to the symbol of the paired `auipc` instruction label.

7. **`sltiu` sign-extends before unsigned compare:** `sltiu rd, rs1, -1` tests `rs1 < 0xFFFFFFFFFFFFFFFF` (always true for non-maximal values), because `-1` sign-extends to all-ones.