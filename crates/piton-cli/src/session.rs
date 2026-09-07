//! Assembling a compiler session.
//!
//! This is the one place in the project where a concrete framework is named.
//! Everything below it works through [`piton_core::framework::Framework`].

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use piton_belay::Belay;
use piton_core::compile::{compile, Compilation};
use piton_core::db::Db;
use piton_core::diag::{Diagnostic, Diagnostics};
use piton_core::framework::{Frameworks, OutputFile};
use piton_core::project::Project;
use piton_core::{builtin, FileId};

/// The frameworks compiled into this binary.
pub fn registry() -> Frameworks {
    Frameworks::new(vec![Box::new(Belay::new())])
}

/// A configured session: the project, its frameworks, and its compilation.
pub struct Session {
    pub project: Project,
    pub frameworks: Frameworks,
    pub compilation: Compilation,
    /// Messages frameworks reported about their own configuration.
    pub notes: Vec<String>,
}

impl Session {
    /// Load the project in `cwd` and compile everything reachable from it.
    pub fn project(cwd: &Path) -> Result<Session> {
        let mut frameworks = registry();
        let loaded = Project::load(cwd, &frameworks);
        let mut diagnostics = loaded.diagnostics;
        let mut notes = Vec::new();
        if let Some(configuration) = &loaded.compilation {
            notes = frameworks.configure(
                &loaded.project,
                configuration,
                &loaded.project.framework_configs,
            );
        }
        let mut db = database(&frameworks);
        db.set_root(&loaded.project.root);
        let entries = entry_files(&mut db, &loaded.project)?;
        let mut compilation = compile(db, entries, &frameworks);
        for diagnostic in diagnostics.iter().cloned().collect::<Vec<Diagnostic>>() {
            compilation.diagnostics.push(diagnostic);
        }
        diagnostics = Diagnostics::default();
        let _ = diagnostics;
        Ok(Session { project: loaded.project, frameworks, compilation, notes })
    }

    /// Compile a specific set of files, using the project only for its root.
    pub fn files(cwd: &Path, paths: &[PathBuf]) -> Result<Session> {
        let mut frameworks = registry();
        let loaded = Project::load(cwd, &frameworks);
        let mut notes = Vec::new();
        if let Some(configuration) = &loaded.compilation {
            notes = frameworks.configure(
                &loaded.project,
                configuration,
                &loaded.project.framework_configs,
            );
        }
        let mut db = database(&frameworks);
        db.set_root(&loaded.project.root);
        let mut entries = Vec::new();
        for path in paths {
            let id = db
                .load(path)
                .map_err(|error| anyhow::anyhow!(error.message))
                .with_context(|| format!("loading {}", path.display()))?;
            entries.push(id);
        }
        let compilation = compile(db, entries, &frameworks);
        Ok(Session { project: loaded.project, frameworks, compilation, notes })
    }

    /// Run every framework's emitter.
    pub fn emit(&self) -> (Vec<OutputFile>, Vec<Diagnostic>) {
        let mut files = Vec::new();
        let mut diagnostics = Vec::new();
        for framework in &self.frameworks.active {
            let emitted = framework.emit(&self.compilation, &self.project);
            files.extend(emitted.files);
            diagnostics.extend(emitted.diagnostics);
        }
        (files, diagnostics)
    }
}

/// A database preloaded with every builtin and framework module.
pub fn database(frameworks: &Frameworks) -> Db {
    let mut db = Db::new();
    for module in builtin::modules().into_iter().chain(frameworks.modules()) {
        db.add_virtual_module(module.name, module.source);
    }
    db
}

/// Resolve a project's entry point to the files a build starts from.
fn entry_files(db: &mut Db, project: &Project) -> Result<Vec<FileId>> {
    let entry = &project.entry;
    if entry.is_file() {
        return Ok(vec![db.load(entry).map_err(|error| anyhow::anyhow!(error.message))?]);
    }
    if entry.is_dir() {
        let index = entry.join("index.pi");
        if index.is_file() {
            return Ok(vec![db.load(&index).map_err(|error| anyhow::anyhow!(error.message))?]);
        }
        // No index: every `.pi` file under the entry is a root.
        let mut entries = Vec::new();
        for path in crate::files::walk(entry) {
            entries.push(db.load(&path).map_err(|error| anyhow::anyhow!(error.message))?);
        }
        if entries.is_empty() {
            bail!("no .pi files found under {}", entry.display());
        }
        return Ok(entries);
    }
    bail!("entry point {} does not exist", entry.display())
}
