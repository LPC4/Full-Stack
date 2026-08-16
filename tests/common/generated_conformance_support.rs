//! Grammar-directed programs and failure reduction for differential tests.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const GENERATED_SEEDS: &[u64] = &[
    0x7a11_ce01_5eed_0001,
    0x7a11_ce01_5eed_0002,
    0x7a11_ce01_5eed_0003,
    0x7a11_ce01_5eed_0004,
];
pub const GENERATED_STEP_LIMIT: u64 = 5_000_000;
pub const PROMOTE_FAILURES_VAR: &str = "UPDATE_GENERATED_CONFORMANCE_FAILURES";
pub const GENERATED_CASES_VAR: &str = "FULL_STACK_GENERATED_CONFORMANCE_CASES";
const MAX_CONFIGURED_CASES: usize = 1_024;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GeneratedProgram {
    pub seed: u64,
    statements: Vec<Statement>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum Statement {
    Arithmetic {
        add: i32,
        mul: i32,
        xor: i32,
    },
    Branch {
        mask: i32,
        pivot: i32,
        left: i32,
        right: i32,
    },
    WhileJoin {
        limit: i32,
        factor: i32,
        bias: i32,
    },
    NestedLoops {
        outer: i32,
        inner: i32,
    },
    ForRange {
        start: i32,
        end: i32,
        skip: i32,
    },
    ArrayFold {
        values: [i32; 4],
    },
}

impl GeneratedProgram {
    pub fn generate(seed: u64) -> Self {
        let mut rng = GeneratorRng(seed);
        let mut statements = Vec::new();
        for index in 0..12 {
            let kind = (index + rng.index(Statement::KINDS)) % Statement::KINDS;
            statements.push(Statement::generate(kind, &mut rng));
        }
        Self { seed, statements }
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            "; generated conformance seed 0x{:016x}\n\
             mix: (value: i32, salt: i32) -> i32 {{\n\
                 mixed: i32 = ((value * 3) + salt) & 255\n\
                 if mixed < 128 {{\n\
                     return mixed + 17\n\
                 }}\n\
                 return mixed - 19\n\
             }}\n\
             \n\
             main: () -> i32 {{\n\
                 acc: i32 = {}\n",
            self.seed,
            self.seed & 255
        );
        for (index, statement) in self.statements.iter().enumerate() {
            statement.render(index, &mut out);
        }
        out.push_str("    return acc & 255\n}\n");
        out
    }

    pub fn minimize<F>(&self, mut reproduces: F) -> Self
    where
        F: FnMut(&Self) -> bool,
    {
        if !reproduces(self) {
            return self.clone();
        }

        let mut reduced = self.clone();
        let mut partitions = 2;
        while !reduced.statements.is_empty() {
            let chunk = reduced.statements.len().div_ceil(partitions);
            let mut removed = false;
            let mut start = 0;
            while start < reduced.statements.len() {
                let end = (start + chunk).min(reduced.statements.len());
                let mut candidate = reduced.clone();
                candidate.statements.drain(start..end);
                if reproduces(&candidate) {
                    reduced = candidate;
                    partitions = 2;
                    removed = true;
                    break;
                }
                start = end;
            }
            if removed {
                continue;
            }
            if partitions >= reduced.statements.len() {
                break;
            }
            partitions = (partitions * 2).min(reduced.statements.len());
        }

        for index in 0..reduced.statements.len() {
            let simplified = reduced.statements[index].simplified();
            if simplified == reduced.statements[index] {
                continue;
            }
            let mut candidate = reduced.clone();
            candidate.statements[index] = simplified;
            if reproduces(&candidate) {
                reduced = candidate;
            }
        }
        reduced
    }

    #[cfg(test)]
    fn contains_nested_loop(&self) -> bool {
        self.statements
            .iter()
            .any(|statement| matches!(statement, Statement::NestedLoops { .. }))
    }
}

pub fn configured_seeds() -> Result<Vec<u64>, String> {
    let Some(value) = std::env::var_os(GENERATED_CASES_VAR) else {
        return Ok(GENERATED_SEEDS.to_vec());
    };
    let value = value
        .into_string()
        .map_err(|_error| format!("{GENERATED_CASES_VAR} must be UTF-8"))?;
    let count = value
        .parse::<usize>()
        .map_err(|error| format!("invalid {GENERATED_CASES_VAR}={value:?}: {error}"))?;
    if !(1..=MAX_CONFIGURED_CASES).contains(&count) {
        return Err(format!(
            "{GENERATED_CASES_VAR} must be in 1..={MAX_CONFIGURED_CASES}, got {count}"
        ));
    }
    let first = GENERATED_SEEDS[0];
    Ok((0..count).map(|index| first + index as u64).collect())
}

impl Statement {
    const KINDS: usize = 6;

    fn generate(kind: usize, rng: &mut GeneratorRng) -> Self {
        match kind {
            0 => Self::Arithmetic {
                add: rng.small(1, 31),
                mul: rng.small(1, 7),
                xor: rng.small(1, 255),
            },
            1 => Self::Branch {
                mask: [3, 7, 15, 31][rng.index(4)],
                pivot: rng.small(1, 15),
                left: rng.small(1, 23),
                right: rng.small(1, 23),
            },
            2 => Self::WhileJoin {
                limit: rng.small(2, 7),
                factor: rng.small(1, 9),
                bias: rng.small(0, 11),
            },
            3 => Self::NestedLoops {
                outer: rng.small(1, 5),
                inner: rng.small(1, 5),
            },
            4 => {
                let start = rng.small(0, 4);
                Self::ForRange {
                    start,
                    end: start + rng.small(2, 7),
                    skip: rng.small(2, 4),
                }
            }
            _ => Self::ArrayFold {
                values: [
                    rng.small(0, 31),
                    rng.small(0, 31),
                    rng.small(0, 31),
                    rng.small(0, 31),
                ],
            },
        }
    }

    fn render(&self, index: usize, out: &mut String) {
        match self {
            Self::Arithmetic { add, mul, xor } => {
                writeln!(out, "    value_{index}: i32 = mix(acc + {add}, {xor})").unwrap();
                writeln!(out, "    acc = ((value_{index} * {mul}) ^ {xor}) & 255").unwrap();
            }
            Self::Branch {
                mask,
                pivot,
                left,
                right,
            } => {
                writeln!(out, "    if (acc & {mask}) < {pivot} {{").unwrap();
                writeln!(out, "        acc = acc + {left}").unwrap();
                out.push_str("    } else {\n");
                writeln!(out, "        acc = acc - {right}").unwrap();
                out.push_str("    }\n    acc = acc & 255\n");
            }
            Self::WhileJoin {
                limit,
                factor,
                bias,
            } => {
                writeln!(out, "    while_i_{index}: i32 = 0").unwrap();
                writeln!(out, "    while_sum_{index}: i32 = acc").unwrap();
                writeln!(out, "    while while_i_{index} < {limit} {{").unwrap();
                writeln!(out, "        if while_i_{index} % 2 == 0 {{").unwrap();
                writeln!(
                    out,
                    "            while_sum_{index} += while_i_{index} * {factor}"
                )
                .unwrap();
                out.push_str("        } else {\n");
                writeln!(out, "            while_sum_{index} -= {bias}").unwrap();
                out.push_str("        }\n");
                writeln!(out, "        while_i_{index} += 1").unwrap();
                out.push_str("    }\n");
                writeln!(out, "    acc = while_sum_{index} & 255").unwrap();
            }
            Self::NestedLoops { outer, inner } => {
                writeln!(out, "    outer_{index}: i32 = 0").unwrap();
                writeln!(out, "    nested_{index}: i32 = 0").unwrap();
                writeln!(out, "    while outer_{index} < {outer} {{").unwrap();
                writeln!(out, "        inner_{index}: i32 = 0").unwrap();
                writeln!(out, "        while inner_{index} < {inner} {{").unwrap();
                writeln!(
                    out,
                    "            nested_{index} += outer_{index} + inner_{index}"
                )
                .unwrap();
                writeln!(out, "            inner_{index} += 1").unwrap();
                out.push_str("        }\n");
                writeln!(out, "        outer_{index} += 1").unwrap();
                out.push_str("    }\n");
                writeln!(out, "    acc = (acc + nested_{index}) & 255").unwrap();
            }
            Self::ForRange { start, end, skip } => {
                writeln!(out, "    range_sum_{index}: i32 = 0").unwrap();
                writeln!(out, "    for range_i_{index} in {start}..{end} {{").unwrap();
                writeln!(out, "        if range_i_{index} % {skip} == 0 {{").unwrap();
                out.push_str("            continue\n        }\n");
                writeln!(out, "        range_sum_{index} += range_i_{index}").unwrap();
                out.push_str("    }\n");
                writeln!(out, "    acc = (acc + range_sum_{index}) & 255").unwrap();
            }
            Self::ArrayFold { values } => {
                writeln!(
                    out,
                    "    items_{index}: i32[4] = [{}, {}, {}, {}]",
                    values[0], values[1], values[2], values[3]
                )
                .unwrap();
                writeln!(out, "    array_sum_{index}: i32 = 0").unwrap();
                writeln!(out, "    for item_{index} in items_{index} {{").unwrap();
                writeln!(out, "        array_sum_{index} += item_{index}").unwrap();
                out.push_str("    }\n");
                writeln!(out, "    acc = (acc ^ array_sum_{index}) & 255").unwrap();
            }
        }
    }

    fn simplified(&self) -> Self {
        match self {
            Self::Arithmetic { .. } => Self::Arithmetic {
                add: 1,
                mul: 1,
                xor: 1,
            },
            Self::Branch { .. } => Self::Branch {
                mask: 1,
                pivot: 1,
                left: 1,
                right: 1,
            },
            Self::WhileJoin { .. } => Self::WhileJoin {
                limit: 1,
                factor: 1,
                bias: 0,
            },
            Self::NestedLoops { .. } => Self::NestedLoops { outer: 1, inner: 1 },
            Self::ForRange { .. } => Self::ForRange {
                start: 0,
                end: 1,
                skip: 2,
            },
            Self::ArrayFold { .. } => Self::ArrayFold {
                values: [0, 0, 0, 0],
            },
        }
    }
}

struct GeneratorRng(u64);

impl GeneratorRng {
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

    fn small(&mut self, low: i32, high: i32) -> i32 {
        low + self.index((high - low + 1) as usize) as i32
    }
}

pub fn failure_artifact_path(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = if std::env::var_os(PROMOTE_FAILURES_VAR).is_some() {
        root.join("tests/fixtures/conformance/generated")
    } else {
        root.join("target/conformance-failures")
    };
    directory.join(format!("{}.hll", sanitize_name(name)))
}

pub fn persist_failure(name: &str, source: &str, details: &str) -> Result<PathBuf, String> {
    let source_path = failure_artifact_path(name);
    let directory = source_path.parent().expect("failure artifact parent");
    std::fs::create_dir_all(directory)
        .map_err(|e| format!("create {}: {e}", directory.display()))?;
    std::fs::write(&source_path, source)
        .map_err(|e| format!("write {}: {e}", source_path.display()))?;
    let detail_path = source_path.with_extension("txt");
    std::fs::write(&detail_path, details)
        .map_err(|e| format!("write {}: {e}", detail_path.display()))?;
    Ok(source_path)
}

pub fn load_regressions() -> Result<Vec<(String, String)>, String> {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance/generated");
    let mut paths = match std::fs::read_dir(&directory) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("hll"))
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("read {}: {error}", directory.display())),
    };
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| format!("invalid regression filename {}", path.display()))?
                .to_owned();
            let source = std::fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            Ok((name, source))
        })
        .collect()
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let first = GeneratedProgram::generate(GENERATED_SEEDS[0]);
        let second = GeneratedProgram::generate(GENERATED_SEEDS[0]);
        assert_eq!(first, second);
        assert_eq!(first.render(), second.render());
    }

    #[test]
    fn minimizer_keeps_only_required_construct() {
        let program = GeneratedProgram::generate(GENERATED_SEEDS[1]);
        assert!(program.contains_nested_loop());
        let reduced = program.minimize(GeneratedProgram::contains_nested_loop);
        assert_eq!(reduced.statements.len(), 1);
        assert!(reduced.contains_nested_loop());
    }

    #[test]
    fn artifact_names_cannot_escape_the_failure_directory() {
        let path = failure_artifact_path("../vm seed:1");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("___vm_seed_1.hll")
        );
    }

    #[test]
    fn default_seeds_are_stable() {
        if std::env::var_os(GENERATED_CASES_VAR).is_none() {
            assert_eq!(configured_seeds().unwrap(), GENERATED_SEEDS);
        }
    }
}
