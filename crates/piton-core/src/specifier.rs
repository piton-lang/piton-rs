//! Writing module specifiers.
//!
//! Resolving a specifier and writing one are one rule read in two directions,
//! so both live beside [`Db`]. Nothing else in the project assembles a path: a
//! specifier written here is only handed out once resolution, asked about that
//! exact text, lands on the file it was written for.

use std::path::{Component, Path, PathBuf};

use crate::db::{canonical, first_module, Db};

/// How a specifier addresses the module it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Style {
    /// `./x` or `../x`, against the importing file's directory.
    Relative,
    /// `/x`, against the project root.
    Root,
    /// `/name/x`, against the library the project named `name`.
    Library(String),
    /// `//x`, against the project's shared root.
    Shared,
    /// `@scope/name`, a module built into the compiler or a framework.
    Builtin,
}

impl Db {
    /// The file a specifier written in `from_dir` leads to, where `exists`
    /// decides which files are there.
    ///
    /// [`Db::resolve`] asks the disk and the editor's buffers. A move that has
    /// not happened yet needs the same answer about a file system that does not
    /// exist yet, which is why the question is a parameter.
    pub fn locate(
        &self,
        from_dir: Option<&Path>,
        spec: &str,
        exists: &dyn Fn(&Path) -> bool,
    ) -> Option<PathBuf> {
        if spec.starts_with('@') {
            return None;
        }
        first_module(&self.bases(from_dir, spec).ok()?, exists)
    }

    /// How an existing specifier addresses its module.
    ///
    /// A rooted specifier names a library only when the root does not answer
    /// it first, because that is the order resolution reads them in.
    pub fn specifier_style(&self, spec: &str, exists: &dyn Fn(&Path) -> bool) -> Style {
        if spec.starts_with('@') {
            return Style::Builtin;
        }
        if spec.starts_with("//") {
            return Style::Shared;
        }
        let Some(rest) = spec.strip_prefix('/') else { return Style::Relative };
        if let Some(root) = self.root() {
            if first_module(&[root.join(rest)], exists).is_some() {
                return Style::Root;
            }
        }
        let name = rest.split('/').next().unwrap_or(rest);
        match self.libraries().iter().any(|(library, _)| library == name) {
            true => Style::Library(name.to_string()),
            false => Style::Root,
        }
    }

    /// Write a specifier in `style` that reaches `target` from `from_dir`.
    ///
    /// `target` is the file itself: `x.pi`, or the `index.pi` of a directory.
    /// `None` when the style cannot reach it, which includes the case where the
    /// text it would write resolves somewhere else.
    pub fn write_specifier(
        &self,
        style: &Style,
        from_dir: &Path,
        target: &Path,
        exists: &dyn Fn(&Path) -> bool,
    ) -> Option<String> {
        let target = canonical(target);
        let from_dir = canonical(from_dir);
        for form in module_forms(&target) {
            let written = match style {
                Style::Relative => relative(&from_dir, &form),
                Style::Root => {
                    self.root().and_then(|root| beneath(&canonical(root), &form)).map(|rest| format!("/{rest}"))
                }
                Style::Library(name) => {
                    self.libraries().iter().find(|(library, _)| library == name).and_then(
                        |(_, path)| {
                            let path = canonical(path);
                            match form == path {
                                true => Some(format!("/{name}")),
                                false => beneath(&path, &form).map(|rest| format!("/{name}/{rest}")),
                            }
                        },
                    )
                }
                Style::Shared => self
                    .shared_root()
                    .and_then(|shared| beneath(&canonical(shared), &form))
                    .map(|rest| format!("//{rest}")),
                Style::Builtin => None,
            };
            let Some(written) = written else { continue };
            if self.locate(Some(&from_dir), &written, exists).as_deref() == Some(target.as_path()) {
                return Some(written);
            }
        }
        None
    }

    /// The specifier a new import of `target`, written in `from_dir`, uses.
    ///
    /// Inside the project root it is relative or rooted, whichever is shorter,
    /// and relative when they tie. Outside the root it goes through the shared
    /// root or the library the file lives in, because a relative path out of a
    /// project is the first thing to break when either side moves.
    pub fn preferred_specifier(
        &self,
        from_dir: &Path,
        target: &Path,
        exists: &dyn Fn(&Path) -> bool,
    ) -> Option<String> {
        let file = canonical(target);
        let under = |base: &Path| file.starts_with(canonical(base));
        if self.root().is_some_and(under) {
            return [Style::Relative, Style::Root]
                .iter()
                .filter_map(|style| self.write_specifier(style, from_dir, target, exists))
                .min_by_key(|written| segments(written));
        }
        let mut styles = Vec::new();
        if self.shared_root().is_some_and(under) {
            styles.push(Style::Shared);
        }
        let library = self
            .libraries()
            .iter()
            .filter(|(_, path)| under(path))
            .max_by_key(|(_, path)| canonical(path).components().count());
        if let Some((name, _)) = library {
            styles.push(Style::Library(name.clone()));
        }
        styles.push(Style::Relative);
        styles.iter().find_map(|style| self.write_specifier(style, from_dir, target, exists))
    }
}

/// How many path components a specifier spells out.
///
/// Each `..` counts, `.` does not, and a library's name counts as the component
/// it is. This is the measure of "shorter" everywhere a specifier is chosen.
pub fn segments(spec: &str) -> usize {
    if spec.starts_with('@') {
        return 1;
    }
    spec.trim_start_matches('/').split('/').filter(|part| !part.is_empty() && *part != ".").count()
}

/// Choose among specifiers that would each import the same symbol.
///
/// Each candidate carries whether its module declares the symbol rather than
/// re-exporting it. The fewest segments win, then the declaring module, then
/// the specifier that sorts first, so the choice never depends on the order
/// the candidates were found in.
pub fn best(candidates: impl IntoIterator<Item = (String, bool)>) -> Option<String> {
    candidates
        .into_iter()
        .min_by(|(a, a_declares), (b, b_declares)| {
            segments(a).cmp(&segments(b)).then(b_declares.cmp(a_declares)).then(a.cmp(b))
        })
        .map(|(spec, _)| spec)
}

/// The paths a module can be written as, in order of preference.
///
/// An `index.pi` is its directory, unless a file beside the directory takes
/// that name, in which case only the explicit `dir/index` reaches it.
fn module_forms(target: &Path) -> Vec<PathBuf> {
    match (target.file_name().and_then(|name| name.to_str()), target.parent()) {
        (Some("index.pi"), Some(directory)) => vec![directory.to_path_buf(), directory.join("index")],
        _ => vec![target.with_extension("")],
    }
}

/// `path` under `base`, written with `/`, or `None` when it is not beneath it.
fn beneath(base: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(base).ok()?;
    let parts: Vec<String> = rest.components().map(text).collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `to` written relative to `from`: `./` when it goes no higher, and bare `../`
/// segments when it does, so `./../` is never produced.
fn relative(from: &Path, to: &Path) -> Option<String> {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = to.components().collect();
    let mut shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    if shared == 0 {
        return None;
    }
    // A module that is the directory the file sits in, or one above it, has no
    // name from inside; it is named from its parent instead.
    if shared == to.len() {
        if shared <= 1 {
            return None;
        }
        shared -= 1;
    }
    let ups = from.len() - shared;
    let tail: Vec<String> = to[shared..].iter().copied().map(text).collect();
    Some(match ups {
        0 => format!("./{}", tail.join("/")),
        _ => format!("{}{}", "../".repeat(ups), tail.join("/")),
    })
}

fn text(component: Component) -> String {
    component.as_os_str().to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of files no other test shares.
    fn tree(files: &[&str]) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let ordinal = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("piton-specifier-{}-{ordinal}", std::process::id()));
        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        root
    }

    fn on_disk(path: &Path) -> bool {
        path.is_file()
    }

    #[test]
    fn counts_segments_the_way_a_reader_does() {
        assert_eq!(segments("./x"), 1);
        assert_eq!(segments("../x"), 2);
        assert_eq!(segments("../../a/b"), 4);
        assert_eq!(segments("/scope/point"), 2);
        assert_eq!(segments("//Tool"), 1);
        assert_eq!(segments("@piton/belay"), 1);
    }

    #[test]
    fn a_relative_specifier_never_starts_with_dot_slash_dot_dot() {
        let root = tree(&["a/b/From.pi", "a/c/To.pi", "a/b/Sibling.pi", "a/b/deeper/Down.pi"]);
        let db = Db::new();
        let from = root.join("a/b");
        let write = |target: &str| {
            db.write_specifier(&Style::Relative, &from, &root.join(target), &on_disk)
        };
        assert_eq!(write("a/b/Sibling.pi").as_deref(), Some("./Sibling"));
        assert_eq!(write("a/b/deeper/Down.pi").as_deref(), Some("./deeper/Down"));
        assert_eq!(write("a/c/To.pi").as_deref(), Some("../c/To"));
    }

    #[test]
    fn a_directory_with_an_index_is_written_as_the_directory() {
        let root = tree(&["main.pi", "tools/index.pi", "clash/index.pi", "clash.pi"]);
        let db = Db::new();
        let write = |target: &str| {
            db.write_specifier(&Style::Relative, &root, &root.join(target), &on_disk)
        };
        assert_eq!(write("tools/index.pi").as_deref(), Some("./tools"));
        // `./clash` means `clash.pi`, so the index has to be named outright.
        assert_eq!(write("clash/index.pi").as_deref(), Some("./clash/index"));
    }

    #[test]
    fn the_index_of_the_directory_a_file_is_in_is_named_from_its_parent() {
        let root = tree(&["point/index.pi", "point/Point.pi"]);
        let db = Db::new();
        let written =
            db.write_specifier(&Style::Relative, &root.join("point"), &root.join("point/index.pi"), &on_disk);
        assert_eq!(written.as_deref(), Some("../point"));
    }

    #[test]
    fn rooted_library_and_shared_styles_each_write_their_own_prefix() {
        let base = tree(&[
            "project/spec/lib/Tool.pi",
            "project/spec/deep/nested/Main.pi",
            "vendor/Widget.pi",
            "shared/parts/Gear.pi",
        ]);
        let mut db = Db::new();
        db.set_root(base.join("project/spec"));
        db.set_libraries(vec![("vendor".to_string(), base.join("vendor"))]);
        db.set_shared_root(Some(base.join("shared")));
        let from = base.join("project/spec/deep/nested");

        let root = db.write_specifier(&Style::Root, &from, &base.join("project/spec/lib/Tool.pi"), &on_disk);
        assert_eq!(root.as_deref(), Some("/lib/Tool"));
        let library = db.write_specifier(
            &Style::Library("vendor".to_string()),
            &from,
            &base.join("vendor/Widget.pi"),
            &on_disk,
        );
        assert_eq!(library.as_deref(), Some("/vendor/Widget"));
        let shared = db.write_specifier(&Style::Shared, &from, &base.join("shared/parts/Gear.pi"), &on_disk);
        assert_eq!(shared.as_deref(), Some("//parts/Gear"));
    }

    #[test]
    fn a_library_path_the_root_would_answer_first_is_not_written() {
        // The root has its own `vendor/Widget`, so `/vendor/Widget` would load
        // that file rather than the library's.
        let base = tree(&["spec/vendor/Widget.pi", "lib/Widget.pi", "spec/Main.pi"]);
        let mut db = Db::new();
        db.set_root(base.join("spec"));
        db.set_libraries(vec![("vendor".to_string(), base.join("lib"))]);
        let written = db.write_specifier(
            &Style::Library("vendor".to_string()),
            &base.join("spec"),
            &base.join("lib/Widget.pi"),
            &on_disk,
        );
        assert_eq!(written, None);
        assert_eq!(db.specifier_style("/vendor/Widget", &on_disk), Style::Root);
    }

    #[test]
    fn a_rooted_specifier_is_a_library_only_when_the_root_does_not_answer() {
        let base = tree(&["spec/Main.pi", "lib/Widget.pi"]);
        let mut db = Db::new();
        db.set_root(base.join("spec"));
        db.set_libraries(vec![("vendor".to_string(), base.join("lib"))]);
        assert_eq!(db.specifier_style("/vendor/Widget", &on_disk), Style::Library("vendor".into()));
        assert_eq!(db.specifier_style("/Main", &on_disk), Style::Root);
        assert_eq!(db.specifier_style("//x", &on_disk), Style::Shared);
        assert_eq!(db.specifier_style("../x", &on_disk), Style::Relative);
        assert_eq!(db.specifier_style("@piton/belay", &on_disk), Style::Builtin);
    }

    #[test]
    fn a_new_import_inside_the_root_takes_the_shorter_style_and_relative_on_a_tie() {
        let base = tree(&[
            "spec/scope/arc/ArcTool.pi",
            "spec/scope/point/index.pi",
            "spec/scope/deep/er/still/File.pi",
            "spec/Top.pi",
        ]);
        let mut db = Db::new();
        db.set_root(base.join("spec"));
        let tie = db.preferred_specifier(
            &base.join("spec/scope/arc"),
            &base.join("spec/scope/point/index.pi"),
            &on_disk,
        );
        assert_eq!(tie.as_deref(), Some("../point"), "`/scope/point` ties, and relative wins");
        let deep = db.preferred_specifier(
            &base.join("spec/scope/deep/er/still"),
            &base.join("spec/Top.pi"),
            &on_disk,
        );
        assert_eq!(deep.as_deref(), Some("/Top"));
    }

    #[test]
    fn a_new_import_outside_the_root_goes_through_the_shared_root_or_library() {
        let base = tree(&["project/spec/Main.pi", "shared/Tool.pi", "vendor/Widget.pi"]);
        let mut db = Db::new();
        db.set_root(base.join("project/spec"));
        db.set_shared_root(Some(base.join("shared")));
        db.set_libraries(vec![("vendor".to_string(), base.join("vendor"))]);
        let from = base.join("project/spec");
        assert_eq!(
            db.preferred_specifier(&from, &base.join("shared/Tool.pi"), &on_disk).as_deref(),
            Some("//Tool")
        );
        assert_eq!(
            db.preferred_specifier(&from, &base.join("vendor/Widget.pi"), &on_disk).as_deref(),
            Some("/vendor/Widget")
        );
    }

    #[test]
    fn a_file_system_that_does_not_exist_yet_can_be_asked_about() {
        // The editor has not moved `Tool.pi` yet; the specifier is written for
        // where it is about to be.
        let root = tree(&["main.pi", "tools/Tool.pi"]);
        let db = Db::new();
        let moved_to = canonical(&root.join("parts/Tool.pi"));
        let after = |path: &Path| path == moved_to || (path.is_file() && !path.ends_with("tools/Tool.pi"));
        let written = db.write_specifier(&Style::Relative, &root, &moved_to, &after);
        assert_eq!(written.as_deref(), Some("./parts/Tool"));
        assert_eq!(db.write_specifier(&Style::Relative, &root, &moved_to, &on_disk), None);
    }

    #[test]
    fn the_best_candidate_is_shortest_then_declaring_then_first() {
        assert_eq!(
            best([("../point/Point".to_string(), true), ("../point".to_string(), false)]).as_deref(),
            Some("../point")
        );
        assert_eq!(
            best([("./b".to_string(), false), ("./a".to_string(), true)]).as_deref(),
            Some("./a")
        );
        assert_eq!(
            best([("./b".to_string(), true), ("./a".to_string(), true)]).as_deref(),
            Some("./a")
        );
    }
}
