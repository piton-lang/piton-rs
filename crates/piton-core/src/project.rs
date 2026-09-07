//! Project configuration, read from `piton.config.pi`.

use std::path::{Path, PathBuf};

use crate::builtin;
use crate::compile::{compile, Compilation};
use crate::db::Db;
use crate::diag::{Diagnostic, Diagnostics};
use crate::framework::Frameworks;
use crate::value::Value;
use crate::FileId;

/// The file a project is configured by.
pub const CONFIG_FILE: &str = "piton.config.pi";

/// A resolved project.
#[derive(Clone, Debug)]
pub struct Project {
    /// Where the build was invoked.
    pub cwd: PathBuf,
    /// The directory holding `piton.config.pi`; every configured path is
    /// resolved against it.
    pub base: PathBuf,
    /// The config file, when there is one.
    pub config_path: Option<PathBuf>,
    /// The source root; absolute imports resolve from here.
    pub root: PathBuf,
    /// The entry point, defaulting to the root.
    pub entry: PathBuf,
    /// One value per configured framework, to be claimed by a plugin.
    pub framework_configs: Vec<Value>,
}

impl Project {
    /// A project with no config file, rooted at `cwd`.
    pub fn bare(cwd: impl Into<PathBuf>) -> Project {
        let cwd = cwd.into();
        Project {
            root: cwd.clone(),
            entry: cwd.clone(),
            base: cwd.clone(),
            cwd,
            config_path: None,
            framework_configs: Vec::new(),
        }
    }

    /// Find `piton.config.pi` in `cwd` or any ancestor.
    pub fn find_config(cwd: &Path) -> Option<PathBuf> {
        let mut dir = Some(cwd);
        while let Some(current) = dir {
            let candidate = current.join(CONFIG_FILE);
            if candidate.is_file() {
                return Some(candidate);
            }
            dir = current.parent();
        }
        None
    }

    /// Load and evaluate the project configuration.
    ///
    /// The configuration is itself a Piton program, so this returns the
    /// compilation as well; frameworks need it to recognise their own anchors.
    pub fn load(cwd: &Path, frameworks: &Frameworks) -> Loaded {
        let mut diagnostics = Diagnostics::default();
        let Some(config_path) = Project::find_config(cwd) else {
            return Loaded { project: Project::bare(cwd), compilation: None, diagnostics };
        };
        let config_dir = config_path.parent().unwrap_or(cwd).to_path_buf();

        let mut db = Db::new();
        db.set_root(&config_dir);
        for module in builtin::modules().into_iter().chain(frameworks.modules()) {
            db.add_virtual_module(module.name, module.source);
        }
        let entry = match db.load(&config_path) {
            Ok(id) => id,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "config",
                    FileId(0),
                    Default::default(),
                    error.message,
                ));
                return Loaded { project: Project::bare(cwd), compilation: None, diagnostics };
            }
        };
        let compilation = compile(db, vec![entry], frameworks);
        diagnostics.extend(compilation.diagnostics.iter().cloned());

        let mut project = Project::bare(&config_dir);
        project.cwd = cwd.to_path_buf();
        project.config_path = Some(config_path);
        if let Some(config) = find_config_anchor(&compilation, entry) {
            if let Some(Value::Str(root)) = config.props.get("root") {
                project.root = config_dir.join(root);
            }
            match config.props.get("entry") {
                Some(Value::Str(entry)) => project.entry = config_dir.join(entry),
                _ => project.entry = project.root.clone(),
            }
            if let Some(Value::List(list)) = config.props.get("frameworks") {
                project.framework_configs = list.items.clone();
            }
        } else {
            diagnostics.push(Diagnostic::error(
                "config",
                entry,
                Default::default(),
                format!(
                    "{CONFIG_FILE} does not declare an anchor implementing `piton-config`; \
                     add `use @piton/config` and `export piton-config Config:`"
                ),
            ));
        }
        Loaded { project, compilation: Some(compilation), diagnostics }
    }
}

/// The result of loading a project configuration.
pub struct Loaded {
    pub project: Project,
    /// The compiled configuration file, when there was one.
    pub compilation: Option<Compilation>,
    pub diagnostics: Diagnostics,
}

/// The anchor in the config file that implements `PitonConfig`.
fn find_config_anchor(
    compilation: &Compilation,
    file: FileId,
) -> Option<crate::value::Anchor> {
    let base = compilation.lookup(builtin::CONFIG_MODULE, builtin::CONFIG_ANCHOR)?;
    let hir = &compilation.analysis.db.file(file).hir;
    for index in 0..hir.anchors.len() {
        let id = compilation.analysis.anchor_id(file, index)?;
        if compilation.analysis.ancestors(id).contains(&base) {
            return compilation.anchor(id).cloned();
        }
    }
    None
}
