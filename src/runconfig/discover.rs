use crate::runconfig::{RunConfig, RunKind};
use crate::workspace::{EntryKind, VPath, WorkspaceFs, WorkspaceTree};
use std::io;

pub fn discover_build_manifests(fs: &dyn WorkspaceFs) -> io::Result<Vec<VPath>> {
    let tree = WorkspaceTree::refresh(fs)?;
    let mut manifests = Vec::new();
    for entry in tree.flatten() {
        if entry.kind == EntryKind::File && entry.path.as_str().ends_with(".build") {
            manifests.push(entry.path.clone());
        }
    }
    manifests.sort();
    Ok(manifests)
}

pub fn run_config_from_manifest(fs: &dyn WorkspaceFs, path: VPath) -> Result<RunConfig, io::Error> {
    let text = String::from_utf8(fs.read_file(&path)?).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not UTF-8: {err}", path.as_str()),
        )
    })?;
    let parsed = build_manifest::parse(&text)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let name = parsed
        .string("title")
        .or_else(|| parsed.string("name"))
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or("run")
                .trim_end_matches(".build")
                .to_owned()
        });
    let kind = match parsed.get("target").unwrap_or("hosted") {
        "kernel" => RunKind::KernelBoot,
        "freestanding" => RunKind::Freestanding,
        _ => RunKind::Hosted,
    };
    let mut config = RunConfig::hosted(name, path);
    config.kind = kind;
    if kind == RunKind::KernelBoot {
        config.run.autorun = String::new();
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryFs;

    #[test]
    fn discovers_build_manifests_sorted() {
        let mut fs = MemoryFs::new();
        fs.write_text(&VPath::parse("b/b.build").expect("path"), "name = \"b\"")
            .expect("write");
        fs.write_text(&VPath::parse("a/a.build").expect("path"), "name = \"a\"")
            .expect("write");

        let manifests = discover_build_manifests(&fs).expect("discover");
        assert_eq!(manifests[0].as_str(), "a/a.build");
        assert_eq!(manifests[1].as_str(), "b/b.build");
    }

    #[test]
    fn creates_kernel_run_config_from_manifest_target() {
        let mut fs = MemoryFs::new();
        let path = VPath::parse("kernel.build").expect("path");
        fs.write_text(
            &path,
            "name = \"kernel\"\ntitle = \"Boot kernel\"\ntarget = \"kernel\"",
        )
        .expect("write");

        let config = run_config_from_manifest(&fs, path).expect("config");
        assert_eq!(config.name, "Boot kernel");
        assert_eq!(config.kind, RunKind::KernelBoot);
    }
}
