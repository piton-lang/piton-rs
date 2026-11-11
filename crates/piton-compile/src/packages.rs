//! Packages, dependencies, and the tethers directory.
//!
//! Piton uses managed vendored dependencies. An installed package is an
//! ordinary part of the project. It is a set of plain files under `tethers/`.
//! The project commits these files to version control. The install removes
//! all traces of the source git repository. The compiler does not fetch
//! anything at compile time. No name resolves through a registry. The files
//! on disk *are* the dependency.
//!
//! The rest of this module protects that choice. The files are ordinary, thus
//! a user can edit them. An update must not overwrite an edited
//! package. Thus the lock file records a digest of each installed file.
//! [`Installed::compare`] answers the one question that the package commands
//! ask before they change a file. Is this package still the package that the
//! install put there?

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use piton_core::{Diagnostic, DiagnosticSink, Span, Value};

use crate::digest;

/// The directory, relative to the project root, that holds installed packages.
pub const TETHERS: &str = "tethers";

/// The directory, relative to the source root, that holds untethered packages.
pub const UNTETHERED: &str = "untethered";

/// The lock file, relative to the project root. The project root is the
/// directory that holds `piton.config.pi`.
pub const LOCK_FILE: &str = ".piton/tether.lock";

/// How the compiler selects the version of a dependency.
///
/// A dependency with no specifier follows the default branch. Only in this
/// case can two installs of the same declaration give different files. All
/// other forms name a fixed version.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Pin {
    /// The latest commit on the primary branch of the repository.
    #[default]
    Default,
    Commit(String),
    Tag(String),
    Branch(String),
}

impl Pin {
    pub fn kind(&self) -> &'static str {
        match self {
            Pin::Default => "default",
            Pin::Commit(_) => "commit",
            Pin::Tag(_) => "tag",
            Pin::Branch(_) => "branch",
        }
    }

    pub fn value(&self) -> Option<&str> {
        match self {
            Pin::Default => None,
            Pin::Commit(value) | Pin::Tag(value) | Pin::Branch(value) => Some(value),
        }
    }

    /// Makes a pin again from the two fields that the lock file stores.
    pub fn from_parts(kind: &str, value: Option<&str>) -> Pin {
        match (kind, value) {
            ("commit", Some(value)) => Pin::Commit(value.to_string()),
            ("tag", Some(value)) => Pin::Tag(value.to_string()),
            ("branch", Some(value)) => Pin::Branch(value.to_string()),
            _ => Pin::Default,
        }
    }
}

impl fmt::Display for Pin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pin::Default => f.write_str("default branch"),
            Pin::Commit(value) => write!(f, "commit {value}"),
            Pin::Tag(value) => write!(f, "tag {value}"),
            Pin::Branch(value) => write!(f, "branch {value}"),
        }
    }
}

/// A dependency declared in a project configuration or by a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// A git repository. This is a URL or a path to a repository on disk.
    pub source: String,
    pub pin: Pin,
    /// The packages to take from the repository. Use this when the repository
    /// has many packages and you need only some. `None` takes all of them.
    pub packages: Option<Vec<String>>,
}

impl Dependency {
    pub fn new(source: impl Into<String>) -> Dependency {
        Dependency {
            source: source.into(),
            pin: Pin::Default,
            packages: None,
        }
    }

    /// The package name for a source when the user gives no other name.
    ///
    /// This is the last path segment of the repository without a `.git`
    /// suffix. A repository with its own configuration can declare packages.
    /// Those declarations replace this name. This name is only for a
    /// repository that does not declare packages.
    pub fn default_name(&self) -> String {
        let trimmed = self.source.trim_end_matches('/');
        // `.` and `..` in a local path give a direction. They do not give a name.
        let last = trimmed
            .rsplit(['/', ':', '\\'])
            .find(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
            .unwrap_or(trimmed);
        last.trim_end_matches(".git").to_string()
    }

    /// True when the source is a directory on disk and not a URL.
    ///
    /// A URL has a scheme (`https://`, `ssh://`, `file://`). Or it uses the
    /// git shorthand `user@host:path`. All other sources are paths. A drive
    /// letter (`C:\lib`) is a path. It is not a host with the name `C`.
    pub fn is_local(&self) -> bool {
        let source = self.source.trim();
        if source.is_empty() || source.contains("://") {
            return false;
        }
        match source.split_once(':') {
            Some((host, _)) => host.contains(['/', '\\']) || host.len() == 1,
            None => true,
        }
    }

    /// The location of a local source. A relative path starts at `base`.
    /// `base` is the directory of the `piton.config.pi` that declared the
    /// source. A `~/` path starts at the home directory. It is None for a URL.
    pub fn local_path(&self, base: &Path) -> Option<PathBuf> {
        if !self.is_local() {
            return None;
        }
        let source = self.source.trim();
        let expanded = match source.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest))?,
            None => PathBuf::from(source),
        };
        Some(crate::module::normalize(&base.join(expanded)))
    }
}

/// A package that this project publishes. A `package` anchor declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDecl {
    /// The install name of the package. It is also the name of the package
    /// directory directly below `tethers/`. It never contains `/`. It never
    /// starts with `@`. Refer to [`validate_name`].
    pub name: String,
    /// The absolute path of the directory that the package publishes.
    pub root: PathBuf,
    pub dependencies: Vec<Dependency>,
}

/// A package that the lock file records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub source: String,
    pub pin: Pin,
    /// The source commit of the files, if the install found it.
    pub commit: Option<String>,
    /// The digest of each installed file, in sorted order. The key is the
    /// path of the file relative to the package directory.
    pub files: BTreeMap<String, String>,
}

/// A comparison of a package on disk with the lock file record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Drift {
    pub modified: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl Drift {
    pub fn is_clean(&self) -> bool {
        self.modified.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }

    /// A short description of the changes, for a diagnostic.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        for (label, list) in [
            ("modified", &self.modified),
            ("added", &self.added),
            ("removed", &self.removed),
        ] {
            if !list.is_empty() {
                parts.push(format!("{} {label}", list.len()));
            }
        }
        parts.join(", ")
    }

    /// All changed paths in sorted order, for a report that lists them.
    pub fn paths(&self) -> Vec<String> {
        let mut all: Vec<String> = self
            .modified
            .iter()
            .chain(&self.added)
            .chain(&self.removed)
            .cloned()
            .collect();
        all.sort();
        all
    }
}

impl Installed {
    /// Compares the files on disk with the files that the install recorded.
    pub fn compare(&self, directory: &Path) -> Drift {
        let current = digest_tree(directory);
        let mut drift = Drift::default();
        for (path, hash) in &self.files {
            match current.get(path) {
                Some(found) if found == hash => {}
                Some(_) => drift.modified.push(path.clone()),
                None => drift.removed.push(path.clone()),
            }
        }
        for path in current.keys() {
            if !self.files.contains_key(path) {
                drift.added.push(path.clone());
            }
        }
        drift.modified.sort();
        drift.added.sort();
        drift.removed.sort();
        drift
    }
}

/// The recorded state of all installed packages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lock {
    pub packages: Vec<Installed>,
}

impl Lock {
    pub fn get(&self, name: &str) -> Option<&Installed> {
        self.packages.iter().find(|entry| entry.name == name)
    }

    /// Records a package. This replaces an earlier entry with the same name.
    pub fn insert(&mut self, entry: Installed) {
        match self
            .packages
            .iter_mut()
            .find(|existing| existing.name == entry.name)
        {
            Some(existing) => *existing = entry,
            None => self.packages.push(entry),
        }
        self.packages.sort_by(|a, b| a.name.cmp(&b.name));
    }

    pub fn remove(&mut self, name: &str) -> Option<Installed> {
        let index = self.packages.iter().position(|entry| entry.name == name)?;
        Some(self.packages.remove(index))
    }

    /// Reads the lock file. A lock file that is not there, or that the code cannot read, is empty.
    ///
    /// A lock file that is not there is not an error. A project can commit a package
    /// without a lock file. Each command that needs the lock file gives its
    /// own message about the requested package.
    pub fn load(project_root: &Path) -> Lock {
        let Ok(text) = std::fs::read_to_string(project_root.join(LOCK_FILE)) else {
            return Lock::default();
        };
        parse_lock(&text).unwrap_or_default()
    }

    pub fn save(&self, project_root: &Path) -> std::io::Result<()> {
        let path = project_root.join(LOCK_FILE);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json())
    }

    /// Serializes the lock file.
    ///
    /// This code writes the JSON by hand and does not use a serializer. The
    /// shape is small and fixed. Also, the order must be stable. A project
    /// commits the lock file. Thus a new order of keys gives an unwanted diff.
    pub fn to_json(&self) -> String {
        if self.packages.is_empty() {
            return "{\n  \"version\": 1,\n  \"packages\": []\n}\n".to_string();
        }
        let mut out = String::from("{\n  \"version\": 1,\n  \"packages\": [\n");
        for (index, entry) in self.packages.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&format!("      \"name\": {},\n", quote(&entry.name)));
            out.push_str(&format!("      \"source\": {},\n", quote(&entry.source)));
            out.push_str(&format!("      \"pin\": {},\n", quote(entry.pin.kind())));
            out.push_str(&format!(
                "      \"pinned\": {},\n",
                entry.pin.value().map(quote).unwrap_or_else(null)
            ));
            out.push_str(&format!(
                "      \"commit\": {},\n",
                entry.commit.as_deref().map(quote).unwrap_or_else(null)
            ));
            out.push_str("      \"files\": {\n");
            for (position, (path, hash)) in entry.files.iter().enumerate() {
                let comma = if position + 1 == entry.files.len() { "" } else { "," };
                out.push_str(&format!("        {}: {}{comma}\n", quote(path), quote(hash)));
            }
            out.push_str("      }\n");
            out.push_str(if index + 1 == self.packages.len() {
                "    }\n"
            } else {
                "    },\n"
            });
        }
        out.push_str("  ]\n}\n");
        out
    }
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn null() -> String {
    "null".to_string()
}

/// Parses the lock file.
///
/// This parser is tolerant on purpose. It ignores an entry that it cannot
/// read. It does not fail on the full file. If it failed, the project can
/// not run a package command. Then a person must repair a generated file by
/// hand.
fn parse_lock(text: &str) -> Option<Lock> {
    let value = json::parse(text)?;
    let packages = value.get("packages")?.as_array()?;
    let mut lock = Lock::default();
    for entry in packages {
        let Some(name) = entry.get("name").and_then(json::Value::as_str) else {
            continue;
        };
        let source = entry
            .get("source")
            .and_then(json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let pin = Pin::from_parts(
            entry.get("pin").and_then(json::Value::as_str).unwrap_or(""),
            entry.get("pinned").and_then(json::Value::as_str),
        );
        let commit = entry
            .get("commit")
            .and_then(json::Value::as_str)
            .map(str::to_string);
        let mut files = BTreeMap::new();
        if let Some(map) = entry.get("files").and_then(json::Value::as_object) {
            for (path, hash) in map {
                if let Some(hash) = hash.as_str() {
                    files.insert(path.clone(), hash.to_string());
                }
            }
        }
        lock.packages.push(Installed {
            name: name.to_string(),
            source,
            pin,
            commit,
            files,
        });
    }
    lock.packages.sort_by(|a, b| a.name.cmp(&b.name));
    Some(lock)
}

/// Makes a digest of each file below a directory. The key is the relative path.
///
/// This includes all files, not only `.pi` files. A package can contain a
/// README or a query file. An edit to such a file is also an edit to the
/// package.
pub fn digest_tree(directory: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let relative = path
                    .strip_prefix(directory)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, digest::sha256_hex(&bytes));
            }
        }
    }
    out
}

/// The reason that a package name is not valid, or `None` if it is valid.
///
/// A name is one directory directly below `tethers/`. Thus it cannot contain
/// `/`. A first `@` is only for the packages that the compiler bundles, for
/// example `@piton/belay`. A name of only dots leaves `tethers/`.
pub fn validate_name(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("a package name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\\') {
        return Some(format!(
            "package name `{name}` contains `/`; a package is one directory under tethers/"
        ));
    }
    if name.starts_with('@') {
        return Some(format!(
            "package name `{name}` starts with `@`, which is reserved for the packages bundled with the compiler"
        ));
    }
    if name.chars().all(|ch| ch == '.') {
        return Some(format!("`{name}` is not a package name"));
    }
    None
}

/// The location of the package files. This is one directory directly below
/// `tethers/`.
pub fn package_directory(project_root: &Path, name: &str) -> PathBuf {
    project_root.join(TETHERS).join(name)
}

/// All installed packages, as found in the tethers directory.
///
/// This function does not read the lock file. Each directory directly below
/// `tethers/` is a package. This is true if it has an `index.pi` or not. A
/// package without an index is still a location for imports of its files.
/// An example is `from my-package/Foo import X`. This is true for a package
/// from `piton tether` and for a package that a person committed by hand.
/// Resolution gives the same answer as the filesystem. Thus a project from a
/// checkout works before a user runs a command in it.
pub fn installed_names(project_root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(project_root.join(TETHERS)) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| validate_name(name).is_none() && !name.starts_with('.'))
        .collect();
    found.sort();
    found
}

/// Resolves a written module path when its first segment names an installed
/// package.
///
/// Returns the path with the install directory in place of the package name.
/// The rest of the path resolves as all other paths do.
pub fn resolve_prefix(project_root: &Path, text: &str) -> Option<PathBuf> {
    let (first, rest) = match text.split_once('/') {
        Some((first, rest)) => (first, Some(rest)),
        None => (text, None),
    };
    if validate_name(first).is_some() || first.starts_with('.') {
        return None;
    }
    let directory = package_directory(project_root, first);
    if !directory.is_dir() {
        return None;
    }
    Some(match rest {
        Some(rest) if !rest.is_empty() => directory.join(rest),
        _ => directory,
    })
}


/// True when a written module path can name a package and not a location.
///
/// Relative and root-absolute paths are locations. An `@` path is a bundled
/// package. All other paths are bare names. An import of a package uses a
/// bare name.
pub fn is_bare_path(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with('.')
        && !text.starts_with('/')
        && !text.starts_with('@')
}

/// A version pin as the source wrote it, before evaluation.
///
/// `@piton/config` gives commit, tag, and branch the type string. Thus
/// `tag: 1.0` pins the tag `1.0`. Evaluation reads an unquoted `1.0` as the
/// number 1. The number does not keep the original text. Thus this code collects
/// the written text from the syntax tree. It uses that text when a pin
/// evaluates to a value that is not a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinLiteral {
    pub key: String,
    pub text: String,
}

/// The version specifiers a dependency accepts.
pub const PIN_KEYS: [&str; 3] = ["commit", "tag", "branch"];

/// Collects the written text of each pin in a `dependencies` value.
pub fn pin_literals(value: &piton_syntax::ast::ValueNode, source: &str) -> Vec<PinLiteral> {
    use piton_syntax::ast::BlockItem;

    fn walk_block(block: &piton_syntax::ast::Block, source: &str, out: &mut Vec<PinLiteral>) {
        for item in &block.items {
            match item {
                BlockItem::Property(property) => {
                    if PIN_KEYS.contains(&property.name.as_str()) {
                        if let Some(inline) = &property.value.inline {
                            let text = source
                                .get(inline.span.start..inline.span.end)
                                .unwrap_or_default()
                                .trim();
                            let text = strip_comment(text);
                            out.push(PinLiteral {
                                key: property.name.clone(),
                                text: unquote(text).to_string(),
                            });
                        }
                    }
                    walk_value(&property.value, source, out);
                }
                BlockItem::ListItem(item) => walk_value(&item.value, source, out),
                _ => {}
            }
        }
    }

    fn walk_value(value: &piton_syntax::ast::ValueNode, source: &str, out: &mut Vec<PinLiteral>) {
        if let Some(block) = &value.block {
            walk_block(block, source, out);
        }
    }

    let mut out = Vec::new();
    walk_value(value, source, &mut out);
    out
}

fn strip_comment(text: &str) -> &str {
    match text.find("//") {
        Some(index) => text[..index].trim_end(),
        None => text,
    }
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
}

/// Reads the `dependencies` list of a configuration or a package anchor.
///
/// A dependency is a URL. A dictionary can follow the URL. The dictionary
/// holds one of `commit`, `tag`, or `branch`. The list compiles flat. A pin
/// block with an indent below a URL becomes the next dictionary item. Thus a
/// dictionary attaches to the dependency before it. A dictionary with no
/// dependency before it is a pin for a dependency without a name.
///
/// More than one specifier for one URL is an error. The specifiers can only
/// disagree. A silent choice between them installs a version that nobody
/// chose.
pub fn read_dependencies(
    value: &Value,
    literals: &[PinLiteral],
    file: &Path,
    span: Span,
    diagnostics: &mut DiagnosticSink,
) -> Vec<Dependency> {
    let mut out: Vec<Dependency> = Vec::new();
    // The number of specifiers that the current dependency has.
    let mut pins_for_last = 0usize;
    let mut used: Vec<bool> = vec![false; literals.len()];
    for item in value.as_list_items() {
        match &item {
            Value::Str(text) => {
                if let Some(source) = text.as_plain() {
                    let source = source.trim();
                    if !source.is_empty() {
                        out.push(Dependency::new(source));
                        pins_for_last = 0;
                    }
                }
            }
            Value::Dict(map) => {
                let Some(last) = out.last_mut() else {
                    diagnostics.push(
                        Diagnostic::warning(
                            "dangling-pin",
                            "a version pin has no dependency above it".to_string(),
                            file,
                            span,
                        )
                        .with_help(
                            "write the repository on its own line and indent the pin beneath it"
                                .to_string(),
                        ),
                    );
                    continue;
                };
                for (key, pinned) in map {
                    if key == "packages" {
                        // The packages to take from the repository.
                        let names: Vec<String> = pinned
                            .as_list_items()
                            .iter()
                            .filter_map(|item| match item {
                                Value::Str(text) => text.as_plain().map(|t| t.trim().to_string()),
                                _ => None,
                            })
                            .filter(|name| !name.is_empty())
                            .collect();
                        last.packages = Some(names);
                        continue;
                    }
                    if !PIN_KEYS.contains(&key.as_str()) {
                        diagnostics.push(
                            Diagnostic::warning(
                                "unknown-pin",
                                format!("`{key}` is not a version specifier"),
                                file,
                                span,
                            )
                            .with_help(
                                "the specifiers are `commit`, `tag`, and `branch`, and `packages` picks which packages to take".to_string(),
                            ),
                        );
                        continue;
                    }
                    let Some(text) = pin_text(key, pinned, literals, &mut used) else {
                        continue;
                    };
                    pins_for_last += 1;
                    if pins_for_last > 1 {
                        diagnostics.push(
                            Diagnostic::error(
                                "multiple-pins",
                                format!(
                                    "`{}` is pinned more than once; give it one of commit, tag, or branch",
                                    last.source
                                ),
                                file,
                                span,
                            )
                            .with_help(
                                "a dependency takes a single commit, tag, or branch".to_string(),
                            ),
                        );
                        continue;
                    }
                    last.pin = match key.as_str() {
                        "commit" => Pin::Commit(text),
                        "tag" => Pin::Tag(text),
                        _ => Pin::Branch(text),
                    };
                }
            }
            _ => {}
        }
    }
    out
}

/// The text of one pin. A string is the evaluated value. All other values
/// are the written text.
fn pin_text(
    key: &str,
    value: &Value,
    literals: &[PinLiteral],
    used: &mut [bool],
) -> Option<String> {
    if let Value::Str(text) = value {
        if let Some(text) = text.as_plain() {
            // Quotes have no sense in a value. No git ref contains a quote.
            // Thus `tag: "v1"` reads as the tag `v1`, as the user intended.
            let text = unquote(text.trim());
            mark_literal(key, text, literals, used);
            return Some(text.to_string());
        }
    }
    let evaluated = match value {
        Value::Number(number) => piton_core::value::format_number(*number),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => return None,
        _ => return None,
    };
    // The first unused literal for this key with the same value.
    for (index, literal) in literals.iter().enumerate() {
        if used[index] || literal.key != key {
            continue;
        }
        let same = match value {
            Value::Number(number) => literal.text.parse::<f64>().ok() == Some(*number),
            _ => literal.text == evaluated,
        };
        if same {
            used[index] = true;
            return Some(literal.text.clone());
        }
    }
    Some(evaluated)
}

fn mark_literal(key: &str, text: &str, literals: &[PinLiteral], used: &mut [bool]) {
    if let Some(index) = literals
        .iter()
        .enumerate()
        .position(|(index, literal)| !used[index] && literal.key == key && literal.text == text)
    {
        used[index] = true;
    }
}

/// A JSON reader for the lock file.
///
/// The compiler writes JSON through `piton-emit`. Before this, it did not
/// read JSON. One generated file with a fixed shape is not a good reason to
/// add a parser dependency to the workspace. Thus this reader reads only
/// that shape.
mod json {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Value>),
        Object(BTreeMap<String, Value>),
    }

    impl Value {
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Object(map) => map.get(key),
                _ => None,
            }
        }

        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::String(text) => Some(text),
                _ => None,
            }
        }

        pub fn as_array(&self) -> Option<&[Value]> {
            match self {
                Value::Array(items) => Some(items),
                _ => None,
            }
        }

        pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
            match self {
                Value::Object(map) => Some(map),
                _ => None,
            }
        }
    }

    pub fn parse(text: &str) -> Option<Value> {
        let bytes: Vec<char> = text.chars().collect();
        let mut at = 0usize;
        let value = value(&bytes, &mut at)?;
        skip_space(&bytes, &mut at);
        (at == bytes.len()).then_some(value)
    }

    fn skip_space(text: &[char], at: &mut usize) {
        while *at < text.len() && text[*at].is_whitespace() {
            *at += 1;
        }
    }

    fn value(text: &[char], at: &mut usize) -> Option<Value> {
        skip_space(text, at);
        match text.get(*at)? {
            '{' => object(text, at),
            '[' => array(text, at),
            '"' => string(text, at).map(Value::String),
            't' => literal(text, at, "true").then_some(Value::Bool(true)),
            'f' => literal(text, at, "false").then_some(Value::Bool(false)),
            'n' => literal(text, at, "null").then_some(Value::Null),
            _ => number(text, at),
        }
    }

    fn literal(text: &[char], at: &mut usize, word: &str) -> bool {
        let matched = text[*at..]
            .iter()
            .take(word.len())
            .eq(word.chars().collect::<Vec<_>>().iter());
        if matched {
            *at += word.len();
        }
        matched
    }

    fn object(text: &[char], at: &mut usize) -> Option<Value> {
        *at += 1;
        let mut map = BTreeMap::new();
        loop {
            skip_space(text, at);
            match text.get(*at)? {
                '}' => {
                    *at += 1;
                    return Some(Value::Object(map));
                }
                ',' => *at += 1,
                '"' => {
                    let key = string(text, at)?;
                    skip_space(text, at);
                    if *text.get(*at)? != ':' {
                        return None;
                    }
                    *at += 1;
                    map.insert(key, value(text, at)?);
                }
                _ => return None,
            }
        }
    }

    fn array(text: &[char], at: &mut usize) -> Option<Value> {
        *at += 1;
        let mut items = Vec::new();
        loop {
            skip_space(text, at);
            match text.get(*at)? {
                ']' => {
                    *at += 1;
                    return Some(Value::Array(items));
                }
                ',' => *at += 1,
                _ => items.push(value(text, at)?),
            }
        }
    }

    fn string(text: &[char], at: &mut usize) -> Option<String> {
        *at += 1;
        let mut out = String::new();
        loop {
            let ch = *text.get(*at)?;
            *at += 1;
            match ch {
                '"' => return Some(out),
                '\\' => {
                    let escape = *text.get(*at)?;
                    *at += 1;
                    match escape {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let hex: String = text.get(*at..*at + 4)?.iter().collect();
                            *at += 4;
                            out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn number(text: &[char], at: &mut usize) -> Option<Value> {
        let start = *at;
        while *at < text.len() && "+-0123456789.eE".contains(text[*at]) {
            *at += 1;
        }
        let raw: String = text[start..*at].iter().collect();
        raw.parse().ok().map(Value::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> Installed {
        Installed {
            name: name.to_string(),
            source: format!("https://example.test/{name}"),
            pin: Pin::Tag("1.0".to_string()),
            commit: Some("abc123".to_string()),
            files: BTreeMap::from([
                ("index.pi".to_string(), "aa".to_string()),
                ("nested/Thing.pi".to_string(), "bb".to_string()),
            ]),
        }
    }

    #[test]
    fn a_lock_file_round_trips() {
        let mut lock = Lock::default();
        lock.insert(entry("beta"));
        lock.insert(entry("alpha"));
        let text = lock.to_json();
        let parsed = parse_lock(&text).expect("parses");
        assert_eq!(parsed, lock);
        // The lock keeps its packages in name order. Thus two commits of a lock file give no diff.
        assert_eq!(
            parsed.packages.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "beta"]
        );
    }

    #[test]
    fn an_empty_lock_file_round_trips() {
        let lock = Lock::default();
        assert_eq!(parse_lock(&lock.to_json()), Some(lock));
    }

    #[test]
    fn a_replaced_entry_does_not_duplicate() {
        let mut lock = Lock::default();
        lock.insert(entry("alpha"));
        let mut second = entry("alpha");
        second.commit = Some("def456".to_string());
        lock.insert(second);
        assert_eq!(lock.packages.len(), 1);
        assert_eq!(lock.packages[0].commit.as_deref(), Some("def456"));
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "piton-packages-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("temp dir");
        directory
    }

    #[test]
    fn a_package_is_one_directory_under_tethers() {
        assert_eq!(
            package_directory(Path::new("/project"), "MyPackage"),
            PathBuf::from("/project/tethers/MyPackage")
        );
    }

    #[test]
    fn a_package_name_has_no_slash_and_no_leading_at() {
        assert!(validate_name("my-package").is_none());
        assert!(validate_name("MyPackage").is_none());
        assert!(validate_name("MyScope/package").is_some());
        assert!(validate_name("@piton/belay").is_some());
        assert!(validate_name("@mine").is_some());
        assert!(validate_name("").is_some());
        assert!(validate_name("..").is_some());
    }

    #[test]
    fn every_directory_under_tethers_is_a_package_even_without_an_index() {
        let root = scratch("installed");
        std::fs::create_dir_all(root.join("tethers/with-index")).expect("dirs");
        std::fs::write(root.join("tethers/with-index/index.pi"), "a: 1\n").expect("write");
        std::fs::create_dir_all(root.join("tethers/no-index/nested")).expect("dirs");
        std::fs::write(root.join("tethers/no-index/Foo.pi"), "a: 1\n").expect("write");
        std::fs::write(root.join("tethers/stray.pi"), "a: 1\n").expect("write");

        assert_eq!(installed_names(&root), vec!["no-index", "with-index"]);
        // Nested directories are contents of the package. They are not packages.
        assert_eq!(
            resolve_prefix(&root, "no-index/Foo"),
            Some(root.join("tethers/no-index/Foo"))
        );
        assert_eq!(
            resolve_prefix(&root, "no-index"),
            Some(root.join("tethers/no-index"))
        );
        assert_eq!(resolve_prefix(&root, "missing/Foo"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_lock_file_is_tether_lock() {
        let root = scratch("lock");
        let mut lock = Lock::default();
        lock.insert(entry("alpha"));
        lock.save(&root).expect("save");
        assert!(root.join(".piton/tether.lock").is_file());
        assert_eq!(Lock::load(&root), lock);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_source_names_the_package_it_installs_as() {
        assert_eq!(
            Dependency::new("https://github.com/piton-lang/piton-rs.git").default_name(),
            "piton-rs"
        );
        assert_eq!(
            Dependency::new("https://github.com/piton-lang/piton-rs/").default_name(),
            "piton-rs"
        );
        assert_eq!(
            Dependency::new("git@github.com:piton-lang/piton-rs.git").default_name(),
            "piton-rs"
        );
    }

    #[test]
    fn a_path_is_local_and_a_url_is_not() {
        for local in ["../lib", "./lib", "lib", "/abs/lib", "~/lib", "C:\\lib"] {
            assert!(Dependency::new(local).is_local(), "{local}");
        }
        for remote in [
            "https://github.com/piton-lang/piton-rs",
            "file:///abs/lib",
            "git@github.com:piton-lang/piton-rs.git",
            "host:repo",
        ] {
            assert!(!Dependency::new(remote).is_local(), "{remote}");
        }
        assert_eq!(
            Dependency::new("../lib").local_path(Path::new("/work/app")),
            Some(PathBuf::from("/work/lib"))
        );
        assert_eq!(
            Dependency::new("/abs/lib").local_path(Path::new("/work/app")),
            Some(PathBuf::from("/abs/lib"))
        );
    }

    #[test]
    fn only_a_bare_path_can_name_a_package() {
        assert!(is_bare_path("my-package"));
        assert!(is_bare_path("my-package/Thing"));
        assert!(!is_bare_path("./my-package"));
        assert!(!is_bare_path("/my-package"));
        assert!(!is_bare_path("@piton/belay"));
        assert!(!is_bare_path(""));
    }


    #[test]
    fn drift_names_what_changed() {
        let recorded = Installed {
            name: "demo".to_string(),
            source: String::new(),
            pin: Pin::Default,
            commit: None,
            files: BTreeMap::from([
                ("kept.pi".to_string(), digest::sha256_hex(b"kept")),
                ("edited.pi".to_string(), digest::sha256_hex(b"before")),
                ("deleted.pi".to_string(), digest::sha256_hex(b"gone")),
            ]),
        };
        let directory = std::env::temp_dir().join(format!("piton-drift-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("temp dir");
        std::fs::write(directory.join("kept.pi"), "kept").expect("write");
        std::fs::write(directory.join("edited.pi"), "after").expect("write");
        std::fs::write(directory.join("new.pi"), "new").expect("write");

        let drift = recorded.compare(&directory);
        assert_eq!(drift.modified, vec!["edited.pi"]);
        assert_eq!(drift.added, vec!["new.pi"]);
        assert_eq!(drift.removed, vec!["deleted.pi"]);
        assert!(!drift.is_clean());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_untouched_package_is_clean() {
        let directory = std::env::temp_dir().join(format!("piton-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("nested")).expect("temp dir");
        std::fs::write(directory.join("index.pi"), "export a: 1\n").expect("write");
        std::fs::write(directory.join("nested/Thing.pi"), "b: 2\n").expect("write");

        let recorded = Installed {
            name: "demo".to_string(),
            source: String::new(),
            pin: Pin::Default,
            commit: None,
            files: digest_tree(&directory),
        };
        assert!(recorded.compare(&directory).is_clean());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
