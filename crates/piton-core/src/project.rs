//! Project configuration, read from `piton.config.pi`.

use std::path::{Path, PathBuf};

use piton_syntax::TextRange;

use crate::builtin;
use crate::compile::{compile, Compilation};
use crate::db::Db;
use crate::diag::{Diagnostic, Diagnostics};
use crate::framework::Frameworks;
use crate::hir;
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
    /// Directories outside the root that rooted imports may name, in the order
    /// they were declared. `/name/rest` is `rest` inside the library `name`.
    pub libraries: Vec<(String, PathBuf)>,
    /// The directory shared imports resolve against. `//rest` is `rest` inside
    /// it, and without one `//` has nothing to mean.
    pub shared_root: Option<PathBuf>,
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
            libraries: Vec::new(),
            shared_root: None,
            framework_configs: Vec::new(),
        }
    }

    /// Whether rooted imports mean anything in this project.
    ///
    /// `/a/b` resolves against the root a configuration declares. Without a
    /// configuration there is no such declaration, and the directory a command
    /// happened to run in — or that an editor happened to open — is not one:
    /// taking it for a root makes the same import mean different files
    /// depending on where the tool was started.
    pub fn has_root(&self) -> bool {
        self.config_path.is_some()
    }

    /// Find `piton.config.pi` in `cwd` or any ancestor.
    ///
    /// This answers "which project am I standing in", which is the question a
    /// command-line invocation asks. An editor opens a directory rather than
    /// standing in one, and has to search downward as well.
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
            return Loaded::bare(cwd);
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
        if let Some((index, config)) = find_config_anchor(&compilation, entry) {
            if let Some(Value::Str(root)) = config.props.get("root") {
                project.root = config_dir.join(root);
                // A root that is not there is not a detail to work around. Every
                // rooted import in the project resolves against it, so silently
                // falling back would turn one mistake in one line into an
                // unresolved import on every file that has one.
                if !project.root.is_dir() {
                    diagnostics.push(Diagnostic::error(
                        "config",
                        entry,
                        property_range(&compilation, entry, index, "root"),
                        format!(
                            "`root` is `{root}`, which is not a directory in this project; a \
                             rooted import such as `/lib/Tool` has nothing to resolve against."
                        ),
                    ));
                }
            }
            match config.props.get("entry") {
                Some(Value::Str(path)) => {
                    project.entry = config_dir.join(path);
                    if !project.entry.exists() {
                        diagnostics.push(Diagnostic::error(
                            "config",
                            entry,
                            property_range(&compilation, entry, index, "entry"),
                            format!(
                                "`entry` is `{path}`, which is not a file or a directory in this \
                                 project; a build has nothing to start from."
                            ),
                        ));
                    }
                }
                // An unconfigured entry is the root itself, which has already
                // been reported on if it is missing.
                _ => project.entry = project.root.clone(),
            }
            if let Some(libraries) = config.props.get("libraries") {
                project.libraries = read_libraries(
                    libraries,
                    &config_dir,
                    entry,
                    property_range(&compilation, entry, index, "libraries"),
                    &mut diagnostics,
                );
            }
            if let Some(shared) = config.props.get("sharedRoot") {
                project.shared_root = read_shared_root(
                    shared,
                    &config_dir,
                    entry,
                    property_range(&compilation, entry, index, "sharedRoot"),
                    &mut diagnostics,
                );
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

impl Loaded {
    /// A project with no configuration at all, rooted at `cwd`.
    pub fn bare(cwd: impl AsRef<Path>) -> Loaded {
        Loaded {
            project: Project::bare(cwd.as_ref()),
            compilation: None,
            diagnostics: Diagnostics::default(),
        }
    }

    /// The configuration's diagnostics, re-anchored onto the files of `db`.
    ///
    /// A diagnostic names its file by an id the database that produced it
    /// issued, and the configuration is compiled in a database of its own.
    /// Translating through the path keeps each message on the file it is
    /// actually about; anything `db` has never heard of lands on the config
    /// file, which is the one place a reader can act on it.
    pub fn diagnostics_in(&self, db: &Db) -> Vec<Diagnostic> {
        let config = self.project.config_path.as_deref().and_then(|path| db.file_id(path));
        self.diagnostics
            .iter()
            .filter_map(|diagnostic| {
                let here = self
                    .compilation
                    .as_ref()
                    .and_then(|it| it.analysis.db.file(diagnostic.file).source.as_path())
                    .and_then(|path| db.file_id(path));
                Some(Diagnostic { file: here.or(config)?, ..diagnostic.clone() })
            })
            .collect()
    }
}

/// Read the `libraries` declaration: a name for each directory outside the root
/// that a rooted import may reach.
///
/// Shared code does not always live under the root that imports it — a library
/// beside several projects is the ordinary way to share anchors between them —
/// and naming it keeps `/name/rest` meaning exactly one thing rather than
/// turning every rooted import into a search.
fn read_libraries(
    value: &Value,
    config_dir: &Path,
    file: FileId,
    range: TextRange,
    diagnostics: &mut Diagnostics,
) -> Vec<(String, PathBuf)> {
    let Value::Dict(entries) = value else {
        diagnostics.push(Diagnostic::error(
            "config",
            file,
            range,
            format!(
                "`libraries` is a {}; it names each directory it adds, as in \
                 `libraries:` then `customLib: ../lib`.",
                value.type_name()
            ),
        ));
        return Vec::new();
    };
    let mut libraries = Vec::new();
    for (name, entry) in entries {
        let Value::Str(path) = entry else {
            diagnostics.push(Diagnostic::error(
                "config",
                file,
                range,
                format!("library `{name}` is a {}; it should be a path.", entry.type_name()),
            ));
            continue;
        };
        // The name becomes the first segment of a rooted path, so it has to be
        // one segment: `/a/b` already means `b` inside `a`.
        if name.is_empty() || name.contains('/') {
            diagnostics.push(Diagnostic::error(
                "config",
                file,
                range,
                format!(
                    "library name `{name}` is not a single path segment; it becomes the first \
                     segment of a rooted import such as `/{name}/Thing`."
                ),
            ));
            continue;
        }
        let resolved = config_dir.join(path);
        if !resolved.is_dir() {
            diagnostics.push(Diagnostic::error(
                "config",
                file,
                range,
                format!(
                    "library `{name}` is `{path}`, which is not a directory; a rooted import \
                     such as `/{name}/Thing` has nothing to resolve against."
                ),
            ));
            continue;
        }
        libraries.push((name.clone(), resolved));
    }
    libraries
}

/// Read the `sharedRoot` declaration: the one directory `//` resolves against.
///
/// Several projects beside one another usually share a single directory rather
/// than a set of separately named ones, and spelling its name into every import
/// says nothing a reader did not already know. `//Thing` names that place
/// without naming it, which keeps a moved or renamed shared directory a change
/// to one line of configuration rather than to every file that imports from it.
fn read_shared_root(
    value: &Value,
    config_dir: &Path,
    file: FileId,
    range: TextRange,
    diagnostics: &mut Diagnostics,
) -> Option<PathBuf> {
    let Value::Str(path) = value else {
        diagnostics.push(Diagnostic::error(
            "config",
            file,
            range,
            format!(
                "`sharedRoot` is a {}; it is one path, as in `sharedRoot: ../shared`.",
                value.type_name()
            ),
        ));
        return None;
    };
    let resolved = config_dir.join(path);
    // Reported for the same reason `root` is: every shared import in the
    // project resolves against this one directory, so a path that is not there
    // turns one mistake into an unresolved import in every file that uses `//`.
    if !resolved.is_dir() {
        diagnostics.push(Diagnostic::error(
            "config",
            file,
            range,
            format!(
                "`sharedRoot` is `{path}`, which is not a directory; a shared import such as \
                 `//Tool` has nothing to resolve against."
            ),
        ));
        return None;
    }
    Some(resolved)
}

/// Where a property of the configuration anchor is written.
///
/// A complaint about `root` belongs on the line that set it, not at the top of
/// the file. A property inherited from a base anchor is not written here at
/// all, so the anchor's own name is the closest thing to point at.
fn property_range(
    compilation: &Compilation,
    file: FileId,
    anchor: usize,
    name: &str,
) -> TextRange {
    let definition = &compilation.analysis.db.file(file).hir.anchors[anchor];
    if let hir::Node::Dict(properties) = &definition.body {
        if let Some(property) = properties.iter().find(|it| it.name == name) {
            return property.range;
        }
    }
    definition.name_range
}

/// The anchor in the config file that implements `PitonConfig`, and where in
/// the file it was declared.
fn find_config_anchor(
    compilation: &Compilation,
    file: FileId,
) -> Option<(usize, crate::value::Anchor)> {
    let base = compilation.lookup(builtin::CONFIG_MODULE, builtin::CONFIG_ANCHOR)?;
    let hir = &compilation.analysis.db.file(file).hir;
    for index in 0..hir.anchors.len() {
        let id = compilation.analysis.anchor_id(file, index)?;
        if compilation.analysis.ancestors(id).contains(&base) {
            return compilation.anchor(id).cloned().map(|anchor| (index, anchor));
        }
    }
    None
}
