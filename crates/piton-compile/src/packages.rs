//! Packages, dependencies, and the tethers directory.
//!
//! Piton's strategy is managed vendored dependencies. An installed package is
//! an ordinary part of the project -- plain files under `tethers/`, committed to
//! version control, with every trace of the git repository it came from
//! removed. Nothing is fetched at compile time and nothing resolves through a
//! registry; what is on disk *is* the dependency.
//!
//! That choice is what the rest of this module exists to protect. Because the
//! files are ordinary, they can be edited, and an edited package must not be
//! silently overwritten by an update. The lock file therefore records a digest
//! of every installed file, and [`Installed::compare`] answers the one question
//! the package commands ask before they touch anything: is this package still
//! the one that was installed?

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use piton_core::{Diagnostic, DiagnosticSink, Span, Value};

use crate::digest;

/// Directory, relative to the project root, that installed packages live in.
pub const TETHERS: &str = "tethers";

/// Directory, relative to the source root, that untethered packages move to.
pub const UNTETHERED: &str = "untethered";

/// The lock file, relative to the project root (the directory holding
/// `piton.config.pi`).
pub const LOCK_FILE: &str = ".piton/tether.lock";

/// How a dependency's version is selected.
///
/// A dependency with no specifier tracks the default branch, which is the only
/// case where installing the same declaration twice can produce different
/// files. Every other form names something fixed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Pin {
    /// The latest commit on the repository's primary branch.
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

    /// Rebuilds a pin from the two fields the lock file stores.
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
    /// A git repository: a URL, or a path to one on disk.
    pub source: String,
    pub pin: Pin,
    /// The packages to take from the repository, when it offers several and
    /// only some are wanted. `None` takes all of them.
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

    /// The package name a source installs under when nothing else names it.
    ///
    /// The last path segment of the repository, with a `.git` suffix removed.
    /// A repository that carries its own configuration overrides this by
    /// declaring packages; this is the fallback for one that does not.
    pub fn default_name(&self) -> String {
        let trimmed = self.source.trim_end_matches('/');
        let last = trimmed
            .rsplit(['/', ':', '\\'])
            .find(|segment| !segment.is_empty())
            .unwrap_or(trimmed);
        last.trim_end_matches(".git").to_string()
    }
}

/// A package this project publishes, declared by a `package` anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDecl {
    /// The name the package installs under, and the directory directly beneath
    /// `tethers/` it lands in. Never contains `/` and never starts with `@`;
    /// see [`validate_name`].
    pub name: String,
    /// The directory whose contents are published, absolute.
    pub root: PathBuf,
    pub dependencies: Vec<Dependency>,
}

/// A package recorded in the lock file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub source: String,
    pub pin: Pin,
    /// The commit the files were taken from, when it could be determined.
    pub commit: Option<String>,
    /// Digest of every installed file, keyed by its path relative to the
    /// package directory, in sorted order.
    pub files: BTreeMap<String, String>,
}

/// What a package on disk looks like next to what the lock file recorded.
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

    /// A short description of what changed, for a diagnostic.
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

    /// Every changed path, sorted, for a report that lists them.
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
    /// Compares the files on disk with the ones recorded at install time.
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

/// The recorded state of every installed package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lock {
    pub packages: Vec<Installed>,
}

impl Lock {
    pub fn get(&self, name: &str) -> Option<&Installed> {
        self.packages.iter().find(|entry| entry.name == name)
    }

    /// Records a package, replacing any earlier entry for the same name.
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

    /// Reads the lock file, treating an absent or unreadable one as empty.
    ///
    /// A missing lock file is not an error: a package can be committed without
    /// one, and the commands that need it say so themselves with a message
    /// about the package they were asked about.
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
    /// Written by hand rather than through a serializer because the shape is
    /// small and fixed, and because the ordering has to be stable: a lock file
    /// is committed, so a rewrite that only reorders keys is a diff nobody
    /// asked for.
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
/// Deliberately forgiving: an entry it cannot make sense of is skipped rather
/// than failing the whole file, because the alternative is a project that
/// cannot run any package command until someone repairs a generated file by
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

/// Digests every file beneath a directory, keyed by relative path.
///
/// Every file, not only `.pi` ones: a package may carry a README or a query
/// file, and an edit to one of those is still an edit to the package.
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

/// Why a package name cannot be used, or `None` when it can.
///
/// A name is one directory directly beneath `tethers/`, so it cannot contain
/// `/`. A leading `@` is reserved for the packages bundled with the compiler,
/// such as `@piton/belay`, and a name made only of dots would climb out of
/// `tethers/` altogether.
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

/// Where a package's files live: one directory directly beneath `tethers/`.
pub fn package_directory(project_root: &Path, name: &str) -> PathBuf {
    project_root.join(TETHERS).join(name)
}

/// Every installed package, discovered from the tethers directory.
///
/// The lock file is not consulted. Every directory directly beneath `tethers/`
/// is a package, whether or not it has an `index.pi`: one without an index is
/// still a location its files are imported through, as in
/// `from my-package/Foo import X`. That holds whether it was installed by
/// `piton tether` or committed by hand, and resolution answering the same
/// question the filesystem does is what keeps a checked-out project working
/// before any command has been run in it.
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

/// Resolves a written module path whose first segment names an installed
/// package.
///
/// Returns the path with the package name replaced by the directory it is
/// installed in, leaving the rest of the path to resolve as any other path
/// does.
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


/// True when a written module path could name a package rather than a location.
///
/// Relative and root-absolute paths are locations, and an `@` path is a bundled
/// package. Everything else is a bare name, which is the form a package is
/// imported by.
pub fn is_bare_path(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with('.')
        && !text.starts_with('/')
        && !text.starts_with('@')
}

/// A version pin exactly as it was written in the source, before evaluation.
///
/// `@piton/config` types commit, tag, and branch as strings, so `tag: 1.0`
/// pins the tag `1.0`. Evaluation reads an unquoted `1.0` as the number 1, and
/// the number has lost the spelling, so the written text is collected from the
/// syntax tree and consulted whenever a pin evaluated to something other than
/// a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinLiteral {
    pub key: String,
    pub text: String,
}

/// The version specifiers a dependency accepts.
pub const PIN_KEYS: [&str; 3] = ["commit", "tag", "branch"];

/// Collects the written text of every pin inside a `dependencies` value.
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

/// Reads the `dependencies` list of a configuration or package anchor.
///
/// A dependency is a URL, optionally followed by a dictionary holding one of
/// `commit`, `tag`, or `branch`. The list compiles flat: a pin block indented
/// beneath a URL arrives as the dictionary item that follows it, so a
/// dictionary attaches to the dependency before it, and one with nothing
/// before it is a pin for a dependency that was never named.
///
/// More than one specifier for one URL is an error: they can only disagree,
/// and choosing between them quietly would install a version nobody chose.
pub fn read_dependencies(
    value: &Value,
    literals: &[PinLiteral],
    file: &Path,
    span: Span,
    diagnostics: &mut DiagnosticSink,
) -> Vec<Dependency> {
    let mut out: Vec<Dependency> = Vec::new();
    // How many specifiers the current dependency has been given.
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
                        // Which of the repository's packages to take.
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

/// The text of one pin: a string as it evaluated, anything else as written.
fn pin_text(
    key: &str,
    value: &Value,
    literals: &[PinLiteral],
    used: &mut [bool],
) -> Option<String> {
    if let Value::Str(text) = value {
        if let Some(text) = text.as_plain() {
            // Quotes carry no meaning in a value, and no git ref contains one,
            // so `tag: "v1"` is read as the tag it was clearly meant to be.
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
    // The first unused literal for this key that reads as the same value.
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
/// The compiler writes JSON through `piton-emit` and has never needed to read
/// it. One generated file with a fixed shape is not enough to justify a parser
/// dependency across the workspace, so this reads exactly that shape.
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
        // Sorted, so committing a lock file twice produces no diff.
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
        // Nested directories are the package's contents, not further packages.
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
