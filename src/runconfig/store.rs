use crate::compilation_pipeline::TargetMode;
use crate::runconfig::config::{
    BuildRef, DebugSpec, FsImageSpec, FsMount, PresentMode, RunConfig, RunKind, RunSpec,
};
use crate::workspace::{VPath, WorkspaceFs};
use std::collections::BTreeMap;
use std::fmt;
use std::io;

const RUN_CONFIG_PATH: &str = ".fullstack/run.toml";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RunConfigStore {
    pub configs: Vec<RunConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunConfigParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for RunConfigParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for RunConfigParseError {}

impl RunConfigStore {
    pub fn load(fs: &dyn WorkspaceFs) -> io::Result<Self> {
        let bytes = fs.read_file(&VPath::parse(RUN_CONFIG_PATH).map_err(invalid_path)?)?;
        let text = String::from_utf8(bytes).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("run.toml is not UTF-8: {err}"),
            )
        })?;
        Self::parse(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    pub fn save(&self, fs: &mut dyn WorkspaceFs) -> io::Result<()> {
        fs.create_dir(&VPath::parse(".fullstack").map_err(invalid_path)?)?;
        fs.write_file(
            &VPath::parse(RUN_CONFIG_PATH).map_err(invalid_path)?,
            self.to_toml().as_bytes(),
        )
    }

    pub fn parse(text: &str) -> Result<Self, RunConfigParseError> {
        let mut sections = Vec::new();
        let mut current: Option<RawConfig> = None;
        let mut current_mount: Option<BTreeMap<String, String>> = None;

        for (idx, line) in text.lines().enumerate() {
            let line_no = idx + 1;
            let line = line
                .split_once('#')
                .map(|(body, _)| body)
                .unwrap_or(line)
                .trim();
            if line.is_empty() {
                continue;
            }
            match line {
                "[[config]]" => {
                    flush_mount(&mut current, &mut current_mount, line_no)?;
                    if let Some(raw) = current.take() {
                        sections.push(raw);
                    }
                    current = Some(RawConfig {
                        first_line: line_no,
                        ..RawConfig::default()
                    });
                }
                "[[config.fs.mount]]" => {
                    if current.is_none() {
                        return Err(parse_error(line_no, "mount appears before [[config]]"));
                    }
                    flush_mount(&mut current, &mut current_mount, line_no)?;
                    current_mount = Some(BTreeMap::new());
                }
                _ => {
                    let (key, value) = parse_key_value(line, line_no)?;
                    if let Some(mount) = current_mount.as_mut() {
                        mount.insert(key, value);
                    } else if let Some(raw) = current.as_mut() {
                        if key == "breakpoint" {
                            raw.breakpoints.push(value);
                        } else {
                            raw.values.insert(key, value);
                        }
                    } else {
                        return Err(parse_error(line_no, "key appears before [[config]]"));
                    }
                }
            }
        }

        flush_mount(
            &mut current,
            &mut current_mount,
            text.lines().count().max(1),
        )?;
        if let Some(raw) = current {
            sections.push(raw);
        }

        let mut configs = Vec::with_capacity(sections.len());
        for raw in sections {
            configs.push(raw.into_config()?);
        }
        Ok(Self { configs })
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        for config in &self.configs {
            out.push_str("[[config]]\n");
            out.push_str(&format!("name = \"{}\"\n", escape(&config.name)));
            out.push_str(&format!("type = \"{}\"\n", kind_label(config.kind)));
            match &config.build {
                BuildRef::Manifest(path) => {
                    out.push_str(&format!("build = \"{}\"\n", escape(path.as_str())));
                }
                BuildRef::SourceWithDefaults { root, .. } => {
                    out.push_str(&format!("root = \"{}\"\n", escape(root.as_str())));
                }
            }
            out.push_str(&format!(
                "present = \"{}\"\n",
                present_label(config.run.present)
            ));
            if !config.run.autorun.is_empty() {
                out.push_str(&format!("autorun = \"{}\"\n", escape(&config.run.autorun)));
            }
            out.push_str(&format!(
                "max_steps_per_frame = {}\n",
                config.run.max_steps_per_frame
            ));
            out.push_str(&format!("debug = {}\n", config.debug.enabled));
            for breakpoint in &config.debug.breakpoints {
                out.push_str(&format!("breakpoint = \"{}\"\n", escape(breakpoint)));
            }
            for mount in &config.fs.mounts {
                out.push('\n');
                out.push_str("[[config.fs.mount]]\n");
                out.push_str(&format!("host = \"{}\"\n", escape(mount.host.as_str())));
                out.push_str(&format!("guest = \"{}\"\n", escape(&mount.guest)));
            }
            out.push('\n');
        }
        out
    }
}

#[derive(Default)]
struct RawConfig {
    values: BTreeMap<String, String>,
    mounts: Vec<BTreeMap<String, String>>,
    breakpoints: Vec<String>,
    first_line: usize,
}

impl RawConfig {
    fn into_config(self) -> Result<RunConfig, RunConfigParseError> {
        let name = required(&self.values, "name", self.first_line)?;
        let kind = parse_kind(
            self.values
                .get("type")
                .map(String::as_str)
                .unwrap_or("hosted"),
            self.first_line,
        )?;
        let build = if let Some(path) = self.values.get("build") {
            VPath::parse(path).map(BuildRef::Manifest).map_err(|err| {
                parse_error(self.first_line, &format!("invalid build path: {err}"))
            })?
        } else if let Some(root) = self.values.get("root") {
            BuildRef::SourceWithDefaults {
                root: VPath::parse(root).map_err(|err| {
                    parse_error(self.first_line, &format!("invalid root path: {err}"))
                })?,
                target: target_for_kind(kind),
            }
        } else {
            return Err(parse_error(self.first_line, "missing `build` or `root`"));
        };
        let present = parse_present(
            self.values
                .get("present")
                .map(String::as_str)
                .unwrap_or("both"),
            self.first_line,
        )?;
        let max_steps_per_frame = self
            .values
            .get("max_steps_per_frame")
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|err| {
                parse_error(
                    self.first_line,
                    &format!("max_steps_per_frame must be a number: {err}"),
                )
            })?
            .unwrap_or(200_000);
        let debug = self
            .values
            .get("debug")
            .is_some_and(|value| value == "true");
        let autorun = self.values.get("autorun").cloned().unwrap_or_default();

        let mut mounts = Vec::new();
        for mount in self.mounts {
            let host = required(&mount, "host", self.first_line)?;
            let guest = required(&mount, "guest", self.first_line)?;
            mounts.push(FsMount {
                host: VPath::parse(host).map_err(|err| {
                    parse_error(self.first_line, &format!("invalid mount host: {err}"))
                })?,
                guest: guest.to_owned(),
            });
        }

        Ok(RunConfig {
            name: name.to_owned(),
            kind,
            build,
            fs: FsImageSpec { mounts },
            run: RunSpec {
                present,
                autorun,
                max_steps_per_frame,
            },
            debug: DebugSpec {
                enabled: debug,
                breakpoints: self.breakpoints,
            },
        })
    }
}

fn flush_mount(
    current: &mut Option<RawConfig>,
    mount: &mut Option<BTreeMap<String, String>>,
    line: usize,
) -> Result<(), RunConfigParseError> {
    if let Some(mount) = mount.take() {
        let Some(raw) = current.as_mut() else {
            return Err(parse_error(line, "mount appears before [[config]]"));
        };
        raw.mounts.push(mount);
    }
    Ok(())
}

fn parse_key_value(line: &str, line_no: usize) -> Result<(String, String), RunConfigParseError> {
    let Some((key, value)) = line.split_once('=') else {
        return Err(parse_error(line_no, "expected key = value"));
    };
    Ok((key.trim().to_owned(), parse_value(value.trim(), line_no)?))
}

fn parse_value(value: &str, line_no: usize) -> Result<String, RunConfigParseError> {
    if let Some(stripped) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Ok(stripped.replace("\\\"", "\"").replace("\\\\", "\\"))
    } else if value == "true" || value == "false" || value.parse::<u64>().is_ok() {
        Ok(value.to_owned())
    } else {
        Err(parse_error(
            line_no,
            "expected quoted string, bool, or number",
        ))
    }
}

fn required<'a>(
    map: &'a BTreeMap<String, String>,
    key: &str,
    line: usize,
) -> Result<&'a str, RunConfigParseError> {
    map.get(key)
        .map(String::as_str)
        .ok_or_else(|| parse_error(line, &format!("missing `{key}`")))
}

fn parse_kind(value: &str, line: usize) -> Result<RunKind, RunConfigParseError> {
    match value {
        "hosted" => Ok(RunKind::Hosted),
        "kernel" => Ok(RunKind::KernelBoot),
        "freestanding" => Ok(RunKind::Freestanding),
        _ => Err(parse_error(line, "unknown run config type")),
    }
}

fn parse_present(value: &str, line: usize) -> Result<PresentMode, RunConfigParseError> {
    match value {
        "terminal" => Ok(PresentMode::Terminal),
        "framebuffer" => Ok(PresentMode::Framebuffer),
        "both" => Ok(PresentMode::Both),
        _ => Err(parse_error(line, "unknown present mode")),
    }
}

fn target_for_kind(kind: RunKind) -> TargetMode {
    match kind {
        RunKind::Hosted => TargetMode::Hosted,
        RunKind::KernelBoot => TargetMode::Kernel,
        RunKind::Freestanding => TargetMode::Freestanding,
    }
}

fn kind_label(kind: RunKind) -> &'static str {
    match kind {
        RunKind::Hosted => "hosted",
        RunKind::KernelBoot => "kernel",
        RunKind::Freestanding => "freestanding",
    }
}

fn present_label(present: PresentMode) -> &'static str {
    match present {
        PresentMode::Terminal => "terminal",
        PresentMode::Framebuffer => "framebuffer",
        PresentMode::Both => "both",
    }
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_error(line: usize, message: &str) -> RunConfigParseError {
    RunConfigParseError {
        line,
        message: message.to_owned(),
    }
}

fn invalid_path(err: crate::workspace::PathError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryFs;

    #[test]
    fn parses_kernel_config_with_mount() {
        let text = r#"
[[config]]
name = "Boot kernel + shell"
type = "kernel"
build = "crates/os-runtime/kernel/kernel.build"
present = "both"
autorun = "ls /bin"
max_steps_per_frame = 200000

[[config.fs.mount]]
host = "programs/user"
guest = "/home/demo"
"#;

        let store = RunConfigStore::parse(text).expect("parse");
        assert_eq!(store.configs.len(), 1);
        assert_eq!(store.configs[0].kind, RunKind::KernelBoot);
        assert_eq!(store.configs[0].fs.mounts[0].guest, "/home/demo");
    }

    #[test]
    fn source_config_round_trips_through_toml() {
        let mut store = RunConfigStore::default();
        store.configs.push(RunConfig {
            build: BuildRef::SourceWithDefaults {
                root: VPath::parse("demo/demo.hll").expect("path"),
                target: TargetMode::Kernel,
            },
            debug: DebugSpec {
                enabled: true,
                breakpoints: vec!["main".to_owned(), "helper+0x10".to_owned()],
            },
            kind: RunKind::KernelBoot,
            ..RunConfig::hosted("Source demo", VPath::parse("unused.build").expect("path"))
        });

        let loaded = RunConfigStore::parse(&store.to_toml()).expect("parse");
        assert_eq!(loaded.configs, store.configs);
    }

    #[test]
    fn saves_and_loads_run_toml_from_workspace() {
        let mut fs = MemoryFs::new();
        let store = RunConfigStore {
            configs: vec![RunConfig::hosted(
                "Run example",
                VPath::parse("programs/example/core/core.build").expect("path"),
            )],
        };

        store.save(&mut fs).expect("save");
        assert!(
            fs.read_file(&VPath::parse(".fullstack/run.toml").expect("path"))
                .is_ok()
        );
        let loaded = RunConfigStore::load(&fs).expect("load");
        assert_eq!(loaded.configs[0].name, "Run example");
    }
}
