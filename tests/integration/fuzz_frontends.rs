use asm_to_binary::{Assembler, parse_asm_source};
use hll_to_ir::{CompileConfig, HllCompiler};
use std::panic::{AssertUnwindSafe, catch_unwind};

const GENERATED_CASES: usize = 8 * 1024;
const MAX_GENERATED_LEN: usize = 16 * 1024;
const RANDOM_INPUT_LEN: usize = 2 * 1024;
const SEEDS: &[&str] = &[
    "",
    "main: () -> i32 { return 0 }\n",
    "value: u8[] = \"hello\"\n",
    "if true { return 1 } else { return 0 }\n",
    "items: i32[4] = [1, 2, 3, 4]\n",
    "type Pair<T> = { left: T, right: T }\n",
    "enum Option<T> { Some(T), None }\n",
    "match value { Option.Some(x) => x, Option.None => 0 }\n",
    "for i in 0..=10 { total += i }\n",
    "asm { addi a0, zero, 1 }\n",
    "text := f\"value = {item + 1}\"\n",
    "fn ((([[[{{{\n",
    "'a\u{fffd}z",
    ".text\n.globl main\nmain:\naddi a0, zero, 0\necall\n",
    ".data\nvalue:\n.quad 0x1234\n.asciz \"hello\\n\"\n",
    ".section .rodata\n.balign 16\nbytes:\n.byte 0, 1, 255\n",
    "loop:\namoadd.d.aqrl a0, a1, (a2)\nbne a0, zero, loop\n",
    "beq a0, a1, missing_label\n",
];
const TOKENS: &[&str] = &[
    "fn", "type", "struct", "enum", "impl", "interface", "const", "external", "export",
    "import", "if", "else", "while", "for", "in", "match", "return", "defer", "new",
    "true", "false", "null", "i8", "i32", "u8", "u64", "f32", "bool", "main", "value",
    "(", ")", "[", "]", "{", "}", "<", ">", ":", ":=", "=", "->", ",", ".", "..",
    "..=", "+", "-", "*", "/", "%", "&", "@", "?", "\n", "; comment\n", "0", "1",
    "0xff", "0b1010", "\"text\"", "'x'", ".text\n", ".data\n", ".globl main\n",
    "addi a0, zero, 1\n", "beq a0, a1, label\n", "label:\n", ".quad 42\n",
];
const INTERESTING_BYTES: &[u8] = b"\0\n\r\t;#'\"()[]{}<>:=+-*/%&@?_,.0129azAZ\x7f\x80\xff";

struct FuzzRng(u64);

impl FuzzRng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn index(&mut self, upper: usize) -> usize {
        (self.next() as usize) % upper
    }
}

fn mutate(seed: &str, rng: &mut FuzzRng) -> String {
    let mut bytes = seed.as_bytes().to_vec();
    bytes.truncate(MAX_GENERATED_LEN);
    let mutations = 1 + rng.index(64);

    for _ in 0..mutations {
        match rng.index(6) {
            0 if bytes.len() < MAX_GENERATED_LEN => {
                let index = rng.index(bytes.len() + 1);
                bytes.insert(index, rng.next() as u8);
            }
            1 if !bytes.is_empty() => {
                let index = rng.index(bytes.len());
                bytes[index] = rng.next() as u8;
            }
            2 if !bytes.is_empty() => {
                let index = rng.index(bytes.len());
                bytes.remove(index);
            }
            3 if !bytes.is_empty() && bytes.len() < MAX_GENERATED_LEN => {
                let start = rng.index(bytes.len());
                let max_chunk = (bytes.len() - start).min(256);
                let chunk_len = 1 + rng.index(max_chunk);
                let chunk = bytes[start..start + chunk_len].to_vec();
                let remaining = MAX_GENERATED_LEN - bytes.len();
                let chunk = &chunk[..chunk.len().min(remaining)];
                let index = rng.index(bytes.len() + 1);
                bytes.splice(index..index, chunk.iter().copied());
            }
            4 if bytes.len() > 1 => {
                let left = rng.index(bytes.len());
                let right = rng.index(bytes.len());
                bytes.swap(left, right);
            }
            _ if !bytes.is_empty() => {
                let index = rng.index(bytes.len());
                bytes[index] = INTERESTING_BYTES[rng.index(INTERESTING_BYTES.len())];
            }
            _ => bytes.push(INTERESTING_BYTES[rng.index(INTERESTING_BYTES.len())]),
        }
    }

    String::from_utf8_lossy(&bytes).into_owned()
}

fn random_bytes(rng: &mut FuzzRng) -> String {
    let len = rng.index(RANDOM_INPUT_LEN + 1);
    let bytes = (0..len).map(|_| rng.next() as u8).collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn token_soup(rng: &mut FuzzRng) -> String {
    let target_len = 1 + rng.index(RANDOM_INPUT_LEN * 2);
    let mut source = String::with_capacity(target_len);
    while source.len() < target_len {
        source.push_str(TOKENS[rng.index(TOKENS.len())]);
        if rng.index(3) == 0 {
            source.push(' ');
        }
    }
    source.truncate(target_len);
    source
}

fn preview(source: &str) -> String {
    source
        .chars()
        .take(160)
        .flat_map(char::escape_default)
        .collect()
}

fn assert_frontends_do_not_panic(case: usize, source: &str) {
    let hll = catch_unwind(AssertUnwindSafe(|| {
        let compiler = HllCompiler::new(CompileConfig::default());
        drop(compiler.compile(source));
    }));
    assert!(
        hll.is_ok(),
        "HLL frontend panicked for fuzz case {case} ({} bytes): {}",
        source.len(),
        preview(source)
    );

    let assembler = catch_unwind(AssertUnwindSafe(|| {
        let instructions = parse_asm_source(Some("<fuzz>"), source);
        drop(Assembler::assemble(&instructions));
    }));
    assert!(
        assembler.is_ok(),
        "assembler frontend panicked for fuzz case {case} ({} bytes): {}",
        source.len(),
        preview(source)
    );
}

#[test]
fn bounded_hostile_inputs_do_not_panic_frontends() {
    let mut seeds = SEEDS.to_vec();
    seeds.extend(
        full_stack::userspace::SOURCE_FILES
            .iter()
            .filter(|(path, _)| path.ends_with(".hll") || path.ends_with(".s"))
            .map(|(_, source)| *source),
    );

    let mut case = 0;
    for seed in &seeds {
        assert_frontends_do_not_panic(case, seed);
        case += 1;
    }
    assert_frontends_do_not_panic(case, &"; comment\n".repeat(65_536));
    case += 1;

    let mut rng = FuzzRng(0x5eed_f011_cafe_babe);
    for _ in 0..GENERATED_CASES {
        let source = match rng.index(4) {
            0 => random_bytes(&mut rng),
            1 => token_soup(&mut rng),
            _ => {
                let seed = seeds[rng.index(seeds.len())];
                mutate(seed, &mut rng)
            }
        };
        assert_frontends_do_not_panic(case, &source);
        case += 1;
    }
}
