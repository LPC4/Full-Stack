use super::{
    AssembledOutput, BinaryOutput, BuildError, BuildExecutor, BuildManifest, BuildObject,
    BuildSource, CompilationPipeline, ConsoleStream, FullStackApp, Layer, PipelineFailure,
    PipelineResult, SessionEngine, SessionEngineError, StageCache, StdlibPolicy, TargetMode, VPath,
    diagnostics, egui, is_asm_path,
};

impl FullStackApp {
    pub(super) fn build_active_document(&mut self, now: f64) {
        let config = self.selected_config().cloned();
        let has_config = config.is_some();
        let target_name = config
            .as_ref()
            .and_then(|config| self.config_root(config))
            .map(|path| path.to_string())
            .unwrap_or_else(|| {
                self.active_path()
                    .map(|path| path.to_string())
                    .unwrap_or_else(|| "active document".to_owned())
            });
        self.terminal
            .start("Build", format!("fsc build {target_name}"));
        self.terminal_open = self.settings.open_terminal_on_run;

        self.refresh_build_target();
        if self.visible_layers().is_some() {
            self.refresh_stage_cache();
        } else {
            self.stage_cache = None;
        }
        let produced_layer = if self.visible_layers().is_some() {
            self.stage_cache.as_ref().map(|cache| {
                let result = &cache.result;
                if result.binary.is_some() {
                    Layer::Elf
                } else if result.asm.is_some() {
                    Layer::Asm
                } else if result.ir.is_some() {
                    Layer::Ir
                } else {
                    Layer::Hll
                }
            })
        } else {
            None
        };
        let preview_failed = self
            .stage_cache
            .as_ref()
            .is_some_and(|cache| cache.result.has_errors() || cache.result.has_failure());

        let mut failed = preview_failed;
        if let Some(config) = config {
            match SessionEngine::new(self.workspace.fs()).build_config(&config) {
                Ok(outcome) => {
                    failed = false;
                    self.status_message = Some("built".to_owned());
                    self.report_linked_build(&outcome);
                }
                Err(err) => {
                    failed = true;
                    self.status_message = Some("build failed".to_owned());
                    // Compile failures the preview already diagnosed render as
                    // caret blocks below; the stringified error would repeat them.
                    if !(matches!(err, SessionEngineError::Build(BuildError::Compile(_)))
                        && preview_failed)
                    {
                        self.terminal.push(ConsoleStream::Error, err.to_string());
                    }
                }
            }
        } else if preview_failed {
            self.status_message = Some("build failed".to_owned());
        } else if let Some(cache) = &self.stage_cache {
            self.status_message = Some("built".to_owned());
            self.terminal.push(
                ConsoleStream::Success,
                build_summary(&cache.result, produced_layer),
            );
        } else {
            self.status_message = Some("build skipped".to_owned());
            self.terminal.push(
                ConsoleStream::Warning,
                "No build manifest was found for the active document.",
            );
        }
        if !has_config || failed {
            self.push_build_diagnostics(failed);
        }
        self.build_glow_start = Some(now);
        self.record_build_lit(produced_layer);
    }

    pub(super) fn push_build_diagnostics(&mut self, failed: bool) {
        use full_stack::compilation_pipeline::DiagnosticLevel;
        let Some(cache) = &self.stage_cache else {
            return;
        };
        let root = self
            .active_build_target
            .as_ref()
            .map(|target| target.root_path.as_str().to_owned());
        for diag in &cache.result.diagnostics {
            let stream = match diag.level {
                DiagnosticLevel::Error => ConsoleStream::Error,
                DiagnosticLevel::Warning => ConsoleStream::Warning,
            };
            let block = match &root {
                Some(root) => diag.render(root),
                None => diag.format_full(),
            };
            let link = match (&diag.span, &root) {
                (Some(span), Some(root)) => Some((root.clone(), span.line as usize)),
                _ => None,
            };
            for (i, line) in block.lines().enumerate() {
                let stream = if i == 0 {
                    stream
                } else {
                    ConsoleStream::Detail
                };
                match &link {
                    Some((path, target)) if i == 0 || line.trim_start().starts_with("-->") => {
                        self.terminal
                            .push_linked(stream, line, path.clone(), *target);
                    }
                    _ => self.terminal.push(stream, line),
                }
            }
        }
        if let Some(error) = &cache.result.failure {
            self.terminal.push(ConsoleStream::Error, error.to_string());
        }
        if failed && cache.result.diagnostics.is_empty() && cache.result.failure.is_none() {
            self.terminal.push(
                ConsoleStream::Error,
                "Build failed without a diagnostic payload.",
            );
        }
    }

    pub(super) fn jump_to_source(&mut self, path: &str, line: usize) {
        let Ok(vpath) = VPath::parse(path) else {
            return;
        };
        // Only reopen when the file is not already active, so the stage cache
        // (and with it the editor's diagnostic markers) survives the jump.
        if self.active_path().as_ref() != Some(&vpath) {
            self.open_path(vpath.clone());
            if self.active_path().as_ref() != Some(&vpath) {
                return;
            }
        }
        self.pending_jump = Some((vpath, line));
    }

    pub(super) fn mark_build_stale(&mut self) {
        self.build_lit_until = None;
        self.built_target = None;
        self.build_glow_start = None;
    }

    pub(super) fn record_build_lit(&mut self, layer: Option<Layer>) {
        self.build_lit_until = layer;
        self.built_target = self.active_build_target.clone();
    }

    // A different program's spine must not read as built.
    pub(super) fn lit_layer_for_active_target(&self) -> Option<Layer> {
        (self.built_target.is_some() && self.built_target == self.active_build_target)
            .then_some(self.build_lit_until)
            .flatten()
    }

    pub(super) fn save_active_document(&mut self) {
        match self.workspace.save_active_document() {
            Ok(()) => {
                self.check_after_save("saved");
            }
            Err(err) => {
                self.status_message = Some(format!("save failed: {err:?}"));
            }
        }
    }

    pub(super) fn save_all_documents(&mut self) {
        match self.workspace.save_all() {
            Ok(()) => {
                self.check_after_save("saved all");
            }
            Err(err) => {
                self.status_message = Some(format!("save all failed: {err:?}"));
            }
        }
    }

    pub(super) fn check_after_save(&mut self, verb: &str) {
        self.stage_cache = None;
        if !self.settings.check_on_save {
            self.status_message = Some(verb.to_owned());
            return;
        }
        self.refresh_build_target();
        self.refresh_stage_cache();
        let (errors, warnings) = self
            .stage_cache
            .as_ref()
            .map(|cache| diagnostics::count_levels(&cache.result.diagnostics))
            .unwrap_or((0, 0));
        self.status_message = Some(match (errors, warnings) {
            (0, 0) => verb.to_owned(),
            (0, w) => format!(
                "{verb} \u{00B7} {}",
                diagnostics::tally(w as u64, "warning")
            ),
            (e, 0) => format!("{verb} \u{00B7} {}", diagnostics::tally(e as u64, "error")),
            (e, w) => format!(
                "{verb} \u{00B7} {}, {}",
                diagnostics::tally(e as u64, "error"),
                diagnostics::tally(w as u64, "warning")
            ),
        });
    }

    pub(super) fn poll_idle_check(&mut self, ctx: &egui::Context, now: f64) {
        const IDLE_CHECK_DELAY_SECS: f64 = 1.0;
        if !self.settings.check_on_idle {
            return;
        }
        let Some(doc) = self.workspace.active_document() else {
            return;
        };
        // Clean buffers match disk; the save path already checked them.
        if !doc.is_dirty() {
            return;
        }
        let fingerprint = {
            use std::hash::{Hash as _, Hasher as _};
            let mut hasher = std::hash::DefaultHasher::new();
            doc.text().hash(&mut hasher);
            hasher.finish()
        };
        match self.idle_seen {
            Some((seen, changed_at)) if seen == fingerprint => {
                if self.idle_checked == fingerprint {
                    return;
                }
                if now - changed_at >= IDLE_CHECK_DELAY_SECS {
                    self.idle_checked = fingerprint;
                    self.stage_cache = None;
                    self.refresh_build_target();
                    self.refresh_stage_cache();
                } else {
                    // No input events arrive while the user pauses; keep a
                    // frame queued so the delayed check actually fires.
                    ctx.request_repaint_after(std::time::Duration::from_millis(250));
                }
            }
            _ => {
                self.idle_seen = Some((fingerprint, now));
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
    }

    pub(super) fn refresh_stage_cache(&mut self) {
        let Some(target) = self.active_build_target.clone() else {
            self.stage_cache = None;
            return;
        };
        let Ok(manifest) = self.workspace.load_manifest(&target.manifest_path) else {
            self.stage_cache = None;
            return;
        };
        let (path, mut source) = match &manifest.root {
            BuildSource::Inline {
                source,
                source_path,
                ..
            } => (
                source_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| target.root_path.as_str().to_owned()),
                source.clone(),
            ),
            BuildSource::Path { path, .. } => {
                let path_text = path.display().to_string();
                let Ok(bytes) = self.workspace.fs().read_file(&target.root_path) else {
                    self.stage_cache = None;
                    return;
                };
                let Ok(source) = String::from_utf8(bytes) else {
                    self.stage_cache = None;
                    return;
                };
                (path_text, source)
            }
        };
        // Prefer the open buffer so check-on-idle sees unsaved edits.
        if let Some(doc) = self.workspace.document(&target.root_path) {
            source = doc.text().to_owned();
        }
        if self
            .stage_cache
            .as_ref()
            .is_some_and(|cache| cache.path == path && cache.source == source)
        {
            return;
        }

        // HLL roots show display stages from run_full; raw asm roots skip the
        // HLL front end and let the preview build below report their errors.
        let mut result = if is_asm_path(&path) {
            PipelineResult {
                diagnostics: Vec::new(),
                lex: None,
                parse: None,
                ir: None,
                asm: None,
                binary: None,
                failure: None,
            }
        } else {
            let mut pipeline = BuildExecutor::pipeline_for(&manifest);
            pipeline.set_write_artifacts(false);
            pipeline.set_current_source_path(Some(path.clone()));
            let mut result = pipeline.run_full(&source, &[]);
            result.binary = None;
            result.failure = None;
            result
        };

        if !result.has_errors() {
            match self.build_preview_binary(&manifest, &path, &source) {
                Ok(binary) => result.binary = Some(binary),
                Err(error) => result.failure = Some(error),
            }
        }
        self.stage_cache = Some(StageCache {
            path,
            source,
            result,
        });
    }

    // Reuses the cached per-mode stdlib and the live root buffer so the preview
    // link matches reality.
    pub(super) fn build_preview_binary(
        &mut self,
        manifest: &BuildManifest,
        path: &str,
        source: &str,
    ) -> Result<BinaryOutput, PipelineFailure> {
        let stdlib_mode = match manifest.stdlib {
            StdlibPolicy::None => None,
            StdlibPolicy::ForTarget => Some(manifest.target),
            StdlibPolicy::Explicit(mode) => Some(mode),
        };

        let mut manifest = manifest.clone();
        // Build from the live editor buffer, not the on-disk file.
        manifest.root = BuildSource::inline_with_path(
            manifest.name.clone(),
            source.to_owned(),
            std::path::PathBuf::from(path),
        );
        manifest.write_artifacts = false;
        // Inject the cached stdlib objects instead of recompiling them each keystroke.
        manifest.stdlib = StdlibPolicy::None;
        if let Some(mode) = stdlib_mode {
            let stdlib = self
                .stdlib_objects_for(mode)
                .ok_or_else(|| PipelineFailure::Build("stdlib build failed".to_owned()))?
                .to_vec();
            manifest.extra_objects = stdlib
                .into_iter()
                .map(|(name, assembled)| BuildObject { name, assembled })
                .collect();
        }

        match BuildExecutor::build(&manifest) {
            Ok(artifacts) => Ok(BinaryOutput {
                assembled: artifacts.linked,
            }),
            Err(BuildError::Compile(
                full_stack::compilation_pipeline::CompilationError::Codegen(error),
            )) => Err(PipelineFailure::Codegen(error)),
            Err(BuildError::Link(error)) => Err(PipelineFailure::Linker(error)),
            Err(error) => Err(PipelineFailure::Build(error.to_string())),
        }
    }

    pub(super) fn build_selected_config(&mut self, now: f64) {
        self.save_dirty_before_run();
        if let Some(config) = self.selected_config().cloned()
            && let Some(root) = self.config_root(&config)
            && self.workspace.open_document(root).is_ok()
        {
            self.reset_layer_for_active_file();
        }
        self.build_active_document(now);
    }

    pub(super) fn active_diagnostic_marks(&self, text: &str) -> Vec<diagnostics::DiagMark> {
        let (Some(cache), Some(target), Some(active)) = (
            &self.stage_cache,
            &self.active_build_target,
            self.active_path(),
        ) else {
            return Vec::new();
        };
        if active != target.root_path || cache.source != text {
            return Vec::new();
        }
        diagnostics::marks_for_source(&cache.result.diagnostics, &cache.source)
    }

    pub(super) fn stdlib_objects_for(
        &mut self,
        mode: TargetMode,
    ) -> Option<&[(String, AssembledOutput)]> {
        if !self.stdlib_objects.iter().any(|(m, _)| *m == mode) {
            match CompilationPipeline::compile_stdlib_objects(mode) {
                Ok(objects) => self.stdlib_objects.push((mode, objects)),
                Err(err) => {
                    self.status_message = Some(format!("stdlib build failed: {err}"));
                    return None;
                }
            }
        }
        self.stdlib_objects
            .iter()
            .find(|(m, _)| *m == mode)
            .map(|(_, objects)| objects.as_slice())
    }
}

pub(super) fn build_summary(result: &PipelineResult, produced_layer: Option<Layer>) -> String {
    let stage = produced_layer
        .map(|layer| layer.name().to_owned())
        .unwrap_or_else(|| "no stage".to_owned());
    let sections = result
        .binary
        .as_ref()
        .map(|binary| binary.assembled.sections_iter().count())
        .unwrap_or(0);
    if sections > 0 {
        format!(
            "Build completed through {stage}; linked {}.",
            diagnostics::tally(sections as u64, "section")
        )
    } else {
        format!("Build completed through {stage}.")
    }
}
