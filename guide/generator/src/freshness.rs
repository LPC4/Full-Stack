use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn newest(path: &Path) -> Option<SystemTime> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok();
    if !metadata.is_dir() {
        return modified;
    }

    std::fs::read_dir(path)
        .ok()?
        .filter_map(|entry| newest(&entry.ok()?.path()))
        .chain(modified)
        .max()
}

pub fn is_stale(marker: &Path, fallback: &Path, sources: &[PathBuf]) -> bool {
    let Some(built) = newest(marker).or_else(|| newest(fallback)) else {
        return true;
    };
    sources
        .iter()
        .filter_map(|path| newest(path))
        .any(|source| source > built)
}

pub fn mark(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, [])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{FileTimes, OpenOptions};
    use std::time::{Duration, UNIX_EPOCH};

    fn set_modified(path: &Path, seconds: u64) {
        let file = OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open fixture to set its modification time");
        file.set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
            .expect("set fixture modification time");
    }

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("full-stack-guide-freshness-{stamp}"));
        std::fs::create_dir_all(&path).expect("create freshness test directory");
        path
    }

    #[test]
    fn unchanged_sources_are_fresh() {
        let root = temp_dir();
        let source = root.join("source.rs");
        let output = root.join("output.wasm");
        std::fs::write(&source, "source").expect("write source fixture");
        std::fs::write(&output, "output").expect("write output fixture");
        set_modified(&source, 1);
        set_modified(&output, 2);

        assert!(!is_stale(&root.join("missing-stamp"), &output, &[source]));
        std::fs::remove_dir_all(root).expect("remove freshness test directory");
    }

    #[test]
    fn changed_sources_are_stale() {
        let root = temp_dir();
        let source = root.join("source.rs");
        let output = root.join("output.wasm");
        std::fs::write(&source, "source").expect("write source fixture");
        std::fs::write(&output, "output").expect("write output fixture");
        set_modified(&output, 1);
        set_modified(&source, 2);

        assert!(is_stale(&root.join("missing-stamp"), &output, &[source]));
        std::fs::remove_dir_all(root).expect("remove freshness test directory");
    }

    #[test]
    fn successful_build_marker_wins_over_an_unchanged_output() {
        let root = temp_dir();
        let source = root.join("source.rs");
        let output = root.join("output.wasm");
        let marker = root.join("build.stamp");
        std::fs::write(&output, "output").expect("write output fixture");
        std::fs::write(&source, "source").expect("write source fixture");
        std::fs::write(&marker, []).expect("write marker fixture");
        set_modified(&output, 1);
        set_modified(&source, 2);
        set_modified(&marker, 3);

        assert!(!is_stale(&marker, &output, &[source]));
        std::fs::remove_dir_all(root).expect("remove freshness test directory");
    }
}
