//! Assembling a compiler session.
//!
//! This is the one place in the project where a concrete framework is named.
//! Everything below it works through [`piton_core::framework::Framework`].

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use piton_belay::Belay;
use piton_core::compile::{compile, Compilation};
use piton_core::db::Db;
use piton_core::diag::Diagnostic;
use piton_core::framework::{Frameworks, OutputFile};
use piton_core::project::{Loaded, Project};
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
    /// True when the configuration is itself the problem.
    ///
    /// `compilation` then holds the configuration file and nothing else. A
    /// project whose `root`, `entry` or libraries do not resolve has not
    /// finished saying where its code is or what a rooted import means, so
    /// there is nothing trustworthy to compile the code against.
    pub misconfigured: bool,
    /// Files compiled because a framework asked for them, not because an
    /// entry point reaches them.
    pub roots: Vec<FileId>,
}

impl Session {
    /// Load the project in `cwd` and compile everything reachable from it.
    pub fn project(cwd: &Path) -> Result<Session> {
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
        if loaded.diagnostics.has_errors() {
            return Session::misconfigured(loaded, frameworks, notes);
        }
        let mut db = database(&frameworks);
        if loaded.project.has_root() {
            db.set_root(&loaded.project.root);
        }
        db.set_libraries(loaded.project.libraries.clone());
        db.set_shared_root(loaded.project.shared_root.clone());
        let mut entries = entry_files(&mut db, &loaded.project)?;
        let mut roots = Vec::new();
        for path in frameworks.roots(&loaded.project) {
            let id = db.load(&path).map_err(|error| anyhow::anyhow!(error.message))?;
            if !entries.contains(&id) {
                entries.push(id);
                roots.push(id);
            }
        }
        let compilation = compile(db, entries, &frameworks);
        Ok(Session {
            project: loaded.project,
            frameworks,
            compilation,
            notes,
            misconfigured: false,
            roots,
        })
    }

    /// A session holding nothing but a configuration that cannot be honoured.
    ///
    /// The configuration has already been compiled — reading it meant
    /// evaluating it — so reporting from that compilation puts each message on
    /// the line that caused it. A bare error string cannot do that, and the
    /// editor and the compiler would then be describing the same mistake
    /// differently.
    fn misconfigured(loaded: Loaded, frameworks: Frameworks, notes: Vec<String>) -> Result<Session> {
        let Some(configuration) = &loaded.compilation else {
            // The file could not be read at all, so there is no compilation for
            // a message to be anchored in.
            let messages: Vec<&str> =
                loaded.diagnostics.iter().map(|it| it.message.as_str()).collect();
            bail!("{}", messages.join("\n"));
        };
        let reported = loaded.diagnostics_in(&configuration.analysis.db);
        let Loaded { project, compilation, .. } = loaded;
        let mut compilation = compilation.expect("just matched as present");
        for diagnostic in reported {
            compilation.diagnostics.push(diagnostic);
        }
        Ok(Session { project, frameworks, compilation, notes, misconfigured: true, roots: Vec::new() })
    }

    /// Compile a specific set of files, using the project only for its root.
    ///
    /// The project is looked for beside the files being checked, not beside the
    /// shell: `piton check some/project/src` should use that project's config,
    /// which is where its `root` and its frameworks are declared.
    pub fn files(cwd: &Path, paths: &[PathBuf]) -> Result<Session> {
        let mut frameworks = registry();
        let search = project_directory(cwd, paths);
        let loaded = Project::load(&search, &frameworks);
        let mut notes = Vec::new();
        if let Some(configuration) = &loaded.compilation {
            notes = frameworks.configure(
                &loaded.project,
                configuration,
                &loaded.project.framework_configs,
            );
        }
        // Named files are still a project's files: they resolve their rooted
        // imports against its root, so a configuration that does not resolve
        // stops this as surely as it stops a build.
        if loaded.diagnostics.has_errors() {
            return Session::misconfigured(loaded, frameworks, notes);
        }
        let mut db = database(&frameworks);
        if loaded.project.has_root() {
            db.set_root(&loaded.project.root);
        }
        db.set_libraries(loaded.project.libraries.clone());
        db.set_shared_root(loaded.project.shared_root.clone());
        let mut entries = Vec::new();
        for path in paths {
            let id = db
                .load(path)
                .map_err(|error| anyhow::anyhow!(error.message))
                .with_context(|| format!("loading {}", path.display()))?;
            entries.push(id);
        }
        let compilation = compile(db, entries, &frameworks);
        Ok(Session {
            project: loaded.project,
            frameworks,
            compilation,
            notes,
            misconfigured: false,
            roots: Vec::new(),
        })
    }

    /// Everything the compilation has to say, ready to report.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.compilation.diagnostics.iter().cloned().collect()
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

/// Where to start looking for `piton.config.pi`.
///
/// The deepest directory that contains every path being compiled, so a project
/// is found from inside it however the command was invoked.
fn project_directory(cwd: &Path, paths: &[PathBuf]) -> PathBuf {
    let mut shared: Option<PathBuf> = None;
    for path in paths {
        let directory = if path.is_dir() { path.clone() } else { path.parent().unwrap_or(path).to_path_buf() };
        let directory = piton_core::db::canonical(&directory);
        shared = Some(match shared {
            None => directory,
            Some(current) => common_ancestor(&current, &directory),
        });
    }
    shared.filter(|path| path.components().count() > 1).unwrap_or_else(|| cwd.to_path_buf())
}

fn common_ancestor(left: &Path, right: &Path) -> PathBuf {
    let mut shared = PathBuf::new();
    for (a, b) in left.components().zip(right.components()) {
        if a != b {
            break;
        }
        shared.push(a);
    }
    shared
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
    bail!("entry point {} does not exist", piton_core::db::canonical(entry).display())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a throwaway project and load it the way `piton build` would.
    fn project(files: &[(&str, &str)]) -> (PathBuf, Session) {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let ordinal = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("piton-cli-test-{}-{ordinal}", std::process::id()));
        for (path, contents) in files {
            let target = root.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, contents).unwrap();
        }
        let session = Session::project(&root).expect("the project loads");
        (root, session)
    }

    const GOOD: &[(&str, &str)] = &[
        ("piton.config.pi", "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n"),
        ("spec/index.pi", "from /Thing export *\n"),
        ("spec/Thing.pi", "export anchor Thing:\n    x: 1\n"),
    ];

    #[test]
    fn a_project_that_resolves_compiles() {
        let (root, session) = project(GOOD);
        assert!(!session.misconfigured);
        assert!(session.diagnostics().is_empty(), "{:?}", session.diagnostics());
        assert!(session.compilation.analysis.db.file_id(&root.join("spec/Thing.pi")).is_some());
    }

    /// The message has to name a file and a line, because that is what the
    /// editor shows for the same mistake. A bare error string cannot.
    #[test]
    fn an_entry_that_is_not_there_is_reported_on_the_config() {
        let (root, session) = project(&[
            (
                "piton.config.pi",
                "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    \
                 entry: ./spec/index.pi\n",
            ),
            ("spec/Thing.pi", "use /nowhere\n\nthing Broken:\n    x: 1\n"),
        ]);
        assert!(session.misconfigured);
        let diagnostics = session.diagnostics();
        assert_eq!(diagnostics.len(), 1, "only the configuration: {diagnostics:?}");
        let config = session.compilation.analysis.db.file_id(&root.join("piton.config.pi"));
        assert_eq!(config, Some(diagnostics[0].file), "reported on the config file");
        assert!(diagnostics[0].message.contains("`entry` is `./spec/index.pi`"), "{diagnostics:?}");
        let text = &session.compilation.analysis.db.file(diagnostics[0].file).text;
        assert!(text[diagnostics[0].range].starts_with("entry:"), "on the line that set it");
        // Nothing was compiled, so the pile of unresolved imports the bad
        // configuration would cause is not reported over the top of it.
        assert!(session.compilation.analysis.db.file_id(&root.join("spec/Thing.pi")).is_none());
    }

    #[test]
    fn a_root_that_is_not_there_is_reported_on_the_config() {
        let (_, session) = project(&[(
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n",
        )]);
        assert!(session.misconfigured);
        let diagnostics = session.diagnostics();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].message.contains("`root` is `./spec`"), "{diagnostics:?}");
    }

    /// Named files resolve their rooted imports against the project root too,
    /// so `piton check one/file.pi` stops for the same reason a build does.
    #[test]
    fn checking_named_files_stops_at_a_broken_configuration() {
        let (root, _) = project(GOOD);
        std::fs::write(
            root.join("piton.config.pi"),
            "use @piton/config\n\nexport piton-config Config:\n    root: ./nowhere\n",
        )
        .unwrap();
        let session = Session::files(&root, &[root.join("spec/Thing.pi")]).expect("loads");
        assert!(session.misconfigured);
        assert!(session.diagnostics().iter().any(|it| it.message.contains("`root` is `./nowhere`")));
    }

    /// A framework may compile a file nothing imports, and a build has to see
    /// it and emit it, while it stays distinguishable from the entry point.
    #[test]
    fn a_framework_root_is_compiled_without_being_reached() {
        let (root, session) = project(&[
            (
                "piton.config.pi",
                "use @piton/config\nuse @piton/belay\n\nexport piton-config Config:\n    \
                 root: ./spec\n    entry: ./spec/index.pi\n\n    frameworks:\n        - {Belay}\n\n\
                 belay-config Belay:\n    codeRoot: ./src\n",
            ),
            ("src/.keep", ""),
            ("spec/index.pi", "export anchor Thing:\n    x: 1\n"),
            (
                "spec/deep/Guide.pi",
                "use @piton/belay\n\nexport self-instruction Guide:\n    description: d\n    prompt: p\n",
            ),
        ]);
        assert!(!session.misconfigured);
        assert!(session.diagnostics().is_empty(), "{:?}", session.diagnostics());
        let guide = session.compilation.analysis.db.file_id(&root.join("spec/deep/Guide.pi"));
        assert_eq!(session.roots, guide.into_iter().collect::<Vec<_>>());
        let (outputs, _) = session.emit();
        let expected = piton_core::db::canonical(&root.join("spec/deep")).join("AGENTS.md");
        assert!(outputs.iter().any(|it| it.path == expected), "{outputs:#?}");
    }

    #[test]
    fn a_library_is_reachable_from_a_build() {
        let (_, session) = project(&[
            (
                "piton.config.pi",
                "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n\n    \
                 libraries:\n        customLib: ./lib\n",
            ),
            ("spec/index.pi", "from /customLib/Tool import Tool\n\na: {Tool.kind}\n"),
            ("lib/Tool.pi", "export anchor Tool:\n    kind: hammer\n"),
        ]);
        assert!(!session.misconfigured);
        assert!(session.diagnostics().is_empty(), "{:?}", session.diagnostics());
    }
}
