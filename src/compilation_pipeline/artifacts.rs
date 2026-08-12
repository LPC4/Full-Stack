use super::{CompilationPipeline, DefaultHasher, Location, OptOptions, Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::hash::{Hash as _, Hasher as _};

impl CompilationPipeline {
    pub fn set_artifact_root(&mut self, root: impl Into<PathBuf>) {
        self.artifact_root = root.into();
    }

    pub fn set_write_artifacts(&mut self, enabled: bool) {
        self.write_artifacts = enabled;
    }

    /// Enable the conservative assembly peephole pass. Off by default, because it
    /// rewrites the token stream and the `.s` text rendered from it.
    pub fn set_peephole(&mut self, enabled: bool) {
        self.peephole = enabled;
    }

    /// Keep hot scalars in callee-saved registers (s2-s11) instead of stack slots.
    /// On by default; off gives the pure stack-slot lowering for shape comparisons.
    pub fn set_register_allocation(&mut self, enabled: bool) {
        self.register_allocation = enabled;
    }

    /// Omit the redundant s0 frame pointer in the RV64 backend. On by default.
    pub fn set_omit_frame_pointer(&mut self, enabled: bool) {
        self.omit_frame_pointer = enabled;
    }

    /// Enable IR-level passes (constant folding, dead-code elimination). Off by
    /// default so IR and assembly goldens stay stable.
    pub fn set_optimize(&mut self, opts: OptOptions) {
        self.optimize = opts;
    }

    pub fn set_artifact_stem(&mut self, stem: Option<String>) {
        *self.artifact_stem.get_mut() = stem.map(|s| sanitize_artifact_component(&s));
    }

    pub(super) fn current_artifact_stem(&self) -> Option<String> {
        self.artifact_stem
            .borrow()
            .clone()
            .or_else(|| self.last_artifact_stem.borrow().clone())
    }

    pub(super) fn default_artifact_stem(
        &self,
        caller: &'static Location<'static>,
        hint: &str,
        seed: &str,
    ) -> String {
        let mut hasher = DefaultHasher::new();
        caller.file().hash(&mut hasher);
        caller.line().hash(&mut hasher);
        caller.column().hash(&mut hasher);
        hint.hash(&mut hasher);
        seed.hash(&mut hasher);
        let file_stem = Path::new(caller.file())
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("artifact");
        format!("{file_stem}_{}_{}", caller.line(), hasher.finish())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn write_text_artifact(
        &self,
        subdir: &str,
        stem: &str,
        extension: &str,
        content: &str,
    ) {
        if !self.write_artifacts {
            return;
        }
        let path = self
            .artifact_root
            .join(subdir)
            .join(format!("{stem}{extension}"));
        if let Some(parent) = path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            log::warn!(
                "failed to create artifact directory `{}`: {err}",
                parent.display()
            );
            return;
        }
        if let Err(err) = fs::write(&path, content) {
            log::warn!("failed to write artifact `{}`: {err}", path.display());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn write_bytes_artifact(
        &self,
        subdir: &str,
        stem: &str,
        extension: &str,
        bytes: &[u8],
    ) {
        if !self.write_artifacts {
            return;
        }
        let path = self
            .artifact_root
            .join(subdir)
            .join(format!("{stem}{extension}"));
        if let Some(parent) = path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            log::warn!(
                "failed to create artifact directory `{}`: {err}",
                parent.display()
            );
            return;
        }
        if let Err(err) = fs::write(&path, bytes) {
            log::warn!("failed to write artifact `{}`: {err}", path.display());
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn write_text_artifact(
        &self,
        _subdir: &str,
        _stem: &str,
        _extension: &str,
        _content: &str,
    ) {
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn write_bytes_artifact(
        &self,
        _subdir: &str,
        _stem: &str,
        _extension: &str,
        _bytes: &[u8],
    ) {
    }
}

pub(super) fn sanitize_artifact_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "artifact".to_owned()
    } else {
        trimmed.to_owned()
    }
}
