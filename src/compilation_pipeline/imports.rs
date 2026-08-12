use super::{
    AssembledOutput, CompilationPipeline, Diagnostic, DiagnosticLevel, Path, PathBuf,
    ResolvedModuleSource, ResolvedModules, TargetMode, fs,
};

impl CompilationPipeline {
    pub(super) fn is_filesystem_import(path: &str) -> bool {
        let candidate = Path::new(path);
        candidate.is_absolute()
            || path.starts_with("./")
            || path.starts_with("../")
            || path.starts_with(".\\")
            || path.starts_with("..\\")
    }

    pub(super) fn is_stdlib_module_for_target(&self, name: &str) -> bool {
        hll_to_ir::stdlib::get_stdlib_modules_for_mode(self.target_mode)
            .iter()
            .any(|(module_name, _)| *module_name == name)
    }

    pub(super) fn stdlib_module_source_for_target(&self, name: &str) -> Option<&'static str> {
        hll_to_ir::stdlib::get_stdlib_modules_for_mode(self.target_mode)
            .into_iter()
            .find_map(|(module_name, source)| (module_name == name).then_some(source))
    }

    pub(super) fn resolve_registry_module_source(&self, name: &str) -> Option<String> {
        self.stdlib_module_source_for_target(name)
            .map(str::to_owned)
            .or_else(|| {
                let resolver = self.module_resolver.as_ref()?;
                resolver(name)
            })
    }

    pub(super) fn resolve_module_source(
        &self,
        path: &str,
        base_source_path: Option<&Path>,
    ) -> Result<Option<ResolvedModuleSource>, String> {
        if Self::is_filesystem_import(path) {
            let raw_path = Path::new(path);
            let candidate = if raw_path.is_absolute() {
                raw_path.to_path_buf()
            } else {
                let base_dir = base_source_path
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                base_dir.join(raw_path)
            };

            // Prefer the embedded table so bundled programs resolve identically on every
            // platform (the browser cannot canonicalize); the virtual source_path re-derives.
            let normalized = normalize_lexical(&candidate);
            if let Some(key) = programs_key(&normalized)
                && let Some(source) = crate::userspace::source_by_path(&key)
            {
                return Ok(Some(ResolvedModuleSource {
                    source: source.to_owned(),
                    source_path: Some(PathBuf::from(&key)),
                    key,
                    is_stdlib: false,
                }));
            }

            // Native fallback: raw on-disk paths outside the bundled `programs/` tree.
            let canonical = candidate.canonicalize().map_err(|err| {
                format!(
                    "cannot resolve import(\"{path}\") from `{}`: {err}",
                    base_source_path
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| ".".to_owned())
                )
            })?;
            let source = fs::read_to_string(&canonical).map_err(|err| {
                format!(
                    "cannot read import(\"{path}\") at `{}`: {err}",
                    canonical.display()
                )
            })?;
            return Ok(Some(ResolvedModuleSource {
                key: canonical.display().to_string(),
                source,
                source_path: Some(canonical),
                is_stdlib: false,
            }));
        }

        let key = Self::module_key(path);
        Ok(self
            .resolve_registry_module_source(key)
            .map(|source| ResolvedModuleSource {
                key: key.to_owned(),
                source,
                // A registry module keeps its repo-relative path so its compiled
                // line table (and the debugger) can point at the real file.
                source_path: self.registry_module_path(key).map(PathBuf::from),
                is_stdlib: self.is_stdlib_module_for_target(key),
            }))
    }

    pub(super) fn registry_module_path(&self, key: &str) -> Option<&'static str> {
        if self.is_stdlib_module_for_target(key) {
            hll_to_ir::stdlib::stdlib_module_source_path(self.target_mode, key)
        } else {
            bundled_module_path(key)
        }
    }

    pub(super) fn resolve_modules(&self, source: &str) -> Result<ResolvedModules, Vec<Diagnostic>> {
        let mut prelude = String::new();
        let mut aliases: std::collections::HashMap<String, hll_to_ir::imports::ModuleAlias> =
            std::collections::HashMap::new();
        // Import aliases are namespace handles, not symbol definitions, so they never
        // conflict with a same-named export such as `klog := import("klog")`.

        // An unparsable root skips resolution; the compiler reports it with a span.
        let Ok(mut local_names) = hll_to_ir::imports::collect_declaration_names(source) else {
            return Ok(ResolvedModules {
                prelude: None,
                aliases: std::collections::HashMap::new(),
            });
        };
        for (alias, _) in hll_to_ir::imports::collect_module_imports(source)
            .map_err(|msg| vec![Diagnostic::new(DiagnosticLevel::Error, msg)])?
        {
            local_names.remove(&alias);
        }
        let mut imported_exports: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        if self.module_resolver.is_some() || self.current_source_path.is_some() {
            let module_imports = hll_to_ir::imports::collect_module_imports(source)
                .map_err(|msg| vec![Diagnostic::new(DiagnosticLevel::Error, msg)])?;
            if self.target_mode == TargetMode::Kernel
                && !module_imports
                    .iter()
                    .any(|(_, path)| Self::module_key(path) == "layout")
            {
                prelude.push_str(os_runtime::kernel::LAYOUT);
                prelude.push('\n');
            }
            for (alias, path) in module_imports {
                if aliases.contains_key(&alias) {
                    return Err(vec![Diagnostic::new(
                        DiagnosticLevel::Error,
                        format!("duplicate module import alias `{alias}`"),
                    )]);
                }
                let resolved = self
                    .resolve_module_source(&path, self.current_source_path.as_deref())
                    .map_err(|msg| vec![Diagnostic::new(DiagnosticLevel::Error, msg)])?
                    .ok_or_else(|| {
                        let key = Self::module_key(&path);
                        vec![Diagnostic::new(
                            DiagnosticLevel::Error,
                            format!("cannot resolve import(\"{path}\"): no module named `{key}`"),
                        )]
                    })?;
                let exports =
                    hll_to_ir::imports::collect_exports(&resolved.source).map_err(|msg| {
                        vec![Diagnostic::new(
                            DiagnosticLevel::Error,
                            format!("failed to read exports of \"{path}\": {msg}"),
                        )]
                    })?;
                // Interfaces and generic templates: prefixed like exports, but not link symbols.
                let namespaced = hll_to_ir::imports::collect_exported_interfaces(&resolved.source)
                    .map_err(|msg| {
                        vec![Diagnostic::new(
                            DiagnosticLevel::Error,
                            format!("failed to read interfaces of \"{path}\": {msg}"),
                        )]
                    })?;
                // Only modules the closure itself compiles are mangled. Stdlib modules are
                // precompiled flat and skipped by the closure, so importing one resolves flat.
                let mangle_this = self.mangle_modules && !resolved.is_stdlib;
                let (prefix, mangled) = if mangle_this {
                    let link_symbols =
                        hll_to_ir::imports::collect_exported_link_symbols(&resolved.source)
                            .map_err(|msg| {
                                vec![Diagnostic::new(
                                    DiagnosticLevel::Error,
                                    format!("failed to read exports of \"{path}\": {msg}"),
                                )]
                            })?;
                    (
                        Self::module_prefix(&resolved.key, resolved.source_path.as_deref()),
                        link_symbols,
                    )
                } else {
                    (String::new(), std::collections::HashSet::new())
                };
                // The flat-namespace collision guard only applies when names stay flat across
                // the whole link set, i.e. when this build is not mangling.
                if !self.mangle_modules {
                    Self::check_import_export_collisions(
                        &format!("{alias} ({path})"),
                        &exports,
                        &local_names,
                        &mut imported_exports,
                    )?;
                }
                prelude.push_str(
                    &hll_to_ir::imports::extract_interface_prefixed(&resolved.source, &prefix)
                        .map_err(|msg| {
                            vec![Diagnostic::new(
                                DiagnosticLevel::Error,
                                format!("failed to extract interface of \"{path}\": {msg}"),
                            )]
                        })?,
                );
                let member_aliases = module_member_aliases(&alias, &resolved.key, &exports);
                let declared = hll_to_ir::imports::collect_declaration_names(&resolved.source)
                    .map_err(|msg| {
                        vec![Diagnostic::new(
                            DiagnosticLevel::Error,
                            format!("failed to read declarations of \"{path}\": {msg}"),
                        )]
                    })?;
                aliases.insert(
                    alias,
                    hll_to_ir::imports::ModuleAlias {
                        prefix,
                        member_aliases,
                        exports,
                        mangled,
                        namespaced,
                        declared,
                    },
                );
            }
        }

        prelude.push_str(&self.source_prelude);

        Ok(ResolvedModules {
            prelude: (!prelude.is_empty()).then_some(prelude),
            aliases,
        })
    }

    pub(super) fn check_import_export_collisions(
        label: &str,
        exports: &std::collections::HashSet<String>,
        local_names: &std::collections::HashSet<String>,
        imported_exports: &mut std::collections::HashMap<String, String>,
    ) -> Result<(), Vec<Diagnostic>> {
        // Sort for deterministic diagnostics across runs (sets are unordered).
        let mut sorted_exports: Vec<&String> = exports.iter().collect();
        sorted_exports.sort();
        for export in &sorted_exports {
            if local_names.contains(*export) {
                return Err(vec![Diagnostic::new(
                    DiagnosticLevel::Error,
                    format!(
                        "import `{label}` exports `{export}`, which conflicts with a top-level declaration in the importing module"
                    ),
                )]);
            }
            if let Some(previous) = imported_exports.get(*export) {
                return Err(vec![Diagnostic::new(
                    DiagnosticLevel::Error,
                    format!(
                        "import `{label}` exports `{export}`, which conflicts with the same export from import `{previous}`"
                    ),
                )]);
            }
        }
        for export in sorted_exports {
            imported_exports.insert(export.clone(), label.to_owned());
        }
        Ok(())
    }
}

/// Bundled HLL modules addressable by `import("name")` in host builds.
pub fn bundled_module_source(name: &str) -> Option<&'static str> {
    os_runtime::module_source(name).or_else(|| crate::userspace::module_source(name))
}

// Repo-relative source path of a bundled module, mirroring `bundled_module_source`.
pub(super) fn bundled_module_path(name: &str) -> Option<&'static str> {
    os_runtime::module_path(name).or_else(|| crate::userspace::module_path(name))
}

pub(super) fn module_member_aliases(
    alias: &str,
    key: &str,
    exports: &std::collections::HashSet<String>,
) -> std::collections::HashMap<String, String> {
    let mut aliases = std::collections::HashMap::new();
    // Sort for deterministic alias resolution (the export set is unordered).
    let mut sorted_exports: Vec<&String> = exports.iter().collect();
    sorted_exports.sort();
    for prefix in [alias, key] {
        let prefix = prefix.trim_end_matches(".hll");
        if prefix.is_empty() {
            continue;
        }
        let prefix = format!("{prefix}_");
        for export in &sorted_exports {
            let Some(short) = export.strip_prefix(&prefix) else {
                continue;
            };
            if short.is_empty() {
                continue;
            }
            match aliases.get(short) {
                Some(existing) if existing != *export => {
                    aliases.remove(short);
                }
                Some(_) => {}
                None => {
                    aliases.insert(short.to_owned(), (*export).clone());
                }
            }
        }
    }
    for export in &sorted_exports {
        if let Some(short) = export.strip_prefix('k')
            && short
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_lowercase())
        {
            aliases
                .entry(short.to_owned())
                .or_insert_with(|| (*export).clone());
        }
    }
    aliases
}

pub(super) fn source_identity(name: &str, source_path: Option<&Path>) -> String {
    source_path
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| name.to_owned())
}

pub(super) fn mark_all_defined_symbols_global(assembled: &mut AssembledOutput) {
    let symbols: Vec<String> = assembled
        .symbols_iter()
        .filter(|(name, _addr)| is_legacy_export_symbol(name))
        .map(|(name, _addr)| name.to_owned())
        .collect();
    for symbol in symbols {
        assembled.mark_entry_global(&symbol);
    }
}

pub(super) fn is_legacy_export_symbol(name: &str) -> bool {
    !name.starts_with('.') && !name.contains("__")
}

pub(super) fn programs_module_resolver(name: &str) -> Option<String> {
    bundled_module_source(name).map(str::to_owned)
}

pub(super) fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// Lets a relative import hit the embedded source table via its programs/... key.
pub(super) fn programs_key(path: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut found = false;
    for component in path.components() {
        if let std::path::Component::Normal(part) = component {
            if !found && part == "programs" {
                found = true;
            }
            if found {
                parts.push(part.to_string_lossy().into_owned());
            }
        }
    }
    found.then(|| parts.join("/"))
}
