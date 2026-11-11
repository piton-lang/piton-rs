//! The declaration of a plugin about itself.
//!
//! The declaration of a plugin is a `piton-plugin` anchor. Thus Piton holds
//! all the data that the CLI can know without a run of the plugin. This data
//! is the name, the commands and their arguments, and the permissions on the
//! machine. It also includes the location of the code. The CLI makes its help and parses
//! arguments from the declaration only. It runs JavaScript only when the user
//! invokes a command of the plugin.

use std::path::{Path, PathBuf};

use piton_compile::config::PluginEntry;
use piton_compile::{prelude, Compilation, Project, Symbol};
use serde_json::{Map, Value as Json};

use crate::bundled;

/// The keyword that declares a plugin.
pub const PLUGIN_KEYWORD: &str = "piton-plugin";
/// The abstract anchor that each declaration implements.
pub const PLUGIN_ANCHOR: &str = "PitonPlugin";

/// The location of the code of a plugin.
#[derive(Debug, Clone)]
pub enum Source {
    /// In the compiler. Each entry is a path in the plugin and its text.
    Bundled(&'static [(&'static str, &'static str)]),
    /// On disk, in the directory of its declaration. The host loads no file
    /// outside that directory.
    Directory(PathBuf),
}

impl Source {
    /// Returns each file below `directory` in the plugin, in sorted order.
    /// Each path is relative to `directory` and uses `/`. Symbolic links are
    /// not in the list.
    pub fn list(&self, directory: &str) -> Vec<String> {
        let prefix = directory.trim_matches('/');
        let mut found: Vec<String> = match self {
            Source::Bundled(files) => files
                .iter()
                .filter_map(|(name, _)| {
                    if prefix.is_empty() {
                        Some(name.to_string())
                    } else {
                        name.strip_prefix(prefix)?.strip_prefix('/').map(str::to_string)
                    }
                })
                .collect(),
            Source::Directory(root) => {
                if prefix.split('/').any(|part| part == "..") {
                    return Vec::new();
                }
                let base = root.join(prefix);
                let mut out = Vec::new();
                let mut stack = vec![base.clone()];
                while let Some(current) = stack.pop() {
                    let Ok(entries) = std::fs::read_dir(&current) else { continue };
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let Ok(metadata) = std::fs::symlink_metadata(&path) else { continue };
                        if metadata.file_type().is_symlink() {
                            continue;
                        }
                        if metadata.is_dir() {
                            stack.push(path);
                        } else if let Ok(relative) = path.strip_prefix(&base) {
                            let parts: Vec<String> = relative
                                .components()
                                .map(|part| part.as_os_str().to_string_lossy().to_string())
                                .collect();
                            out.push(parts.join("/"));
                        }
                    }
                }
                out
            }
        };
        found.sort();
        found
    }

    /// The text of a file in the plugin, by its path relative to the plugin.
    /// An example is `src/index.ts`.
    pub fn read(&self, path: &str) -> Option<String> {
        match self {
            Source::Bundled(files) => files
                .iter()
                .find(|(name, _)| *name == path)
                .map(|(_, text)| text.to_string()),
            Source::Directory(directory) => {
                if path.split('/').any(|part| part == "..") {
                    return None;
                }
                std::fs::read_to_string(directory.join(path)).ok()
            }
        }
    }
}

/// The permissions of a plugin on the machine. The host refuses all actions
/// that this list does not give.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capabilities {
    /// The programs that it can run, by name. The host finds them on PATH.
    pub process: Vec<String>,
    /// True if it can read files in the project.
    pub read_files: bool,
    /// True if it can write temporary files. The host removes them when the
    /// command ends.
    pub temporary_files: bool,
    /// The paths that its build can write, as globs relative to the project
    /// root.
    pub outputs: Vec<String>,
    /// True if it can ask the user to choose on a terminal.
    pub interactive: bool,
    /// The files that it can propose to change, as globs relative to the
    /// project root. The host applies the edits when the user agrees.
    pub edits: Vec<String>,
    /// True if the plugin can start a new project. It can then write new
    /// files into a directory that it names. It can also tether the
    /// dependencies of that project.
    pub scaffold: bool,
}

impl Capabilities {
    /// The outputs as a matcher. `*` stays in one directory. `**` goes across
    /// directories. A plugin with no outputs can write nothing.
    pub fn output_globs(&self) -> globset::GlobSet {
        globs(&self.outputs)
    }

    /// The files that it can propose to change, as a matcher.
    pub fn edit_globs(&self) -> globset::GlobSet {
        globs(&self.edits)
    }
}

fn globs(patterns: &[String]) -> globset::GlobSet {
    {
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in patterns {
            if let Ok(glob) = globset::GlobBuilder::new(pattern).literal_separator(true).build() {
                builder.add(glob);
            }
        }
        builder.build().unwrap_or_else(|_| globset::GlobSet::empty())
    }
}

/// The form of an argument.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArgKind {
    /// The flag is present or not present.
    Flag,
    String,
    Number,
}

/// One argument of a plugin command.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgSpec {
    pub name: String,
    pub help: Option<String>,
    pub positional: bool,
    pub multiple: bool,
    pub kind: ArgKind,
    pub choices: Vec<String>,
    pub short: Option<char>,
    pub minimum: Option<f64>,
    pub conflicts: Vec<String>,
    /// The value of the argument when the user does not give it.
    pub default: Option<String>,
    /// For the last positional argument. It takes all values after it, also
    /// values that look like options. The plugin gives them to a program.
    pub trailing: bool,
    /// For an option. True when the user can give it without a value. The
    /// plugin then receives an empty string.
    pub optional_value: bool,
}

/// One command that a plugin adds to the CLI.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandSpec {
    pub name: String,
    /// The group word of the command, for example `belay` for `piton belay
    /// slice`.
    pub group: Option<String>,
    pub about: Option<String>,
    pub help: Option<String>,
    pub arguments: Vec<ArgSpec>,
    /// True if the command runs without a project, as `piton init` does. It
    /// runs in each directory. It ignores the configuration there.
    pub standalone: bool,
}

/// A plugin, as its declaration tells it.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub name: String,
    pub description: Option<String>,
    /// The start module of its behaviour, relative to the plugin.
    pub entry: String,
    pub source: Source,
    /// The package that holds its Piton files, for a bundled plugin.
    pub module_root: Option<String>,
    /// The abstract anchor that the settings of a project for the plugin
    /// implement.
    pub settings: Option<String>,
    pub commands: Vec<CommandSpec>,
    pub capabilities: Capabilities,
    /// The file of its declaration, for diagnostics.
    pub declared_in: PathBuf,
    pub bundled: bool,
    /// The plugins that it requires. It uses their exports.
    pub requires: Vec<String>,
    /// Names that its Piton files export. Their values exist only when the
    /// plugin writes its output.
    pub placeholders: Vec<String>,
    /// True if it has a part in `piton build`.
    pub build: bool,
    /// True if it inspects projects before commands run.
    pub inspect: bool,
    /// True if it names the anchors that it reaches itself. `piton reach` and
    /// the editor count them as roots, together with the entry.
    pub reach: bool,
}

/// The marker that a placeholder evaluates to. It is the name between
/// private-use brackets. No prose contains these brackets.
pub fn placeholder_marker(name: &str) -> String {
    format!("\u{e000}{name}\u{e001}")
}

/// A problem in a declaration, with the file of the declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub file: PathBuf,
    pub message: String,
}

/// All plugins of a project. First come the bundled plugins. Then come the
/// plugins that its configuration declares, in the order of the list. This
/// function omits and reports a declaration that is not well-formed.
pub fn discover(project: &Project) -> (Vec<Manifest>, Vec<Problem>) {
    let mut manifests = Vec::new();
    let mut problems = Vec::new();
    for plugin in bundled::PLUGINS {
        match bundled_manifest(plugin) {
            Ok(manifest) => manifests.push(manifest),
            Err(message) => problems.push(Problem {
                file: PathBuf::from(plugin.declaration),
                message,
            }),
        }
    }
    for entry in &project.plugins {
        if !is_declaration(entry) {
            continue;
        }
        let directory = entry.file.parent().map(Path::to_path_buf).unwrap_or_default();
        match read(&entry.properties, Source::Directory(directory), None, &entry.file, false) {
            Ok(manifest) => {
                if manifests.iter().any(|known: &Manifest| known.name == manifest.name) {
                    problems.push(Problem {
                        file: entry.file.clone(),
                        message: format!("a plugin named `{}` is already loaded", manifest.name),
                    });
                } else {
                    manifests.push(manifest);
                }
            }
            Err(message) => problems.push(Problem {
                file: entry.file.clone(),
                message: format!("plugin `{}`: {message}", entry.name),
            }),
        }
    }

    // Two plugins cannot add the same command. If they can, the order of the
    // list decides which command runs. Thus this function reports and omits
    // the command of the later plugin.
    let mut seen: Vec<(String, String)> = Vec::new();
    for manifest in &mut manifests {
        let mut kept = Vec::new();
        for command in std::mem::take(&mut manifest.commands) {
            if let Some((_, owner)) = seen.iter().find(|(name, _)| *name == command.name) {
                problems.push(Problem {
                    file: manifest.declared_in.clone(),
                    message: format!(
                        "plugin `{}` adds the command `{}`, which plugin `{owner}` already adds",
                        manifest.name, command.name
                    ),
                });
            } else {
                seen.push((command.name.clone(), manifest.name.clone()));
                kept.push(command);
            }
        }
        manifest.commands = kept;
    }

    // Some plugins cannot load. The project does not have the requirements
    // of such a plugin, or the requirements make a circle. This function
    // reports and omits such a plugin. It also omits each plugin that
    // requires it.
    loop {
        let names: Vec<String> = manifests.iter().map(|manifest| manifest.name.clone()).collect();
        let broken = manifests.iter().position(|manifest| {
            manifest.requires.iter().any(|required| !names.contains(required))
                || requires_itself(&manifests, &manifest.name)
        });
        let Some(index) = broken else { break };
        let manifest = manifests.remove(index);
        let message = match manifest.requires.iter().find(|required| !names.contains(*required)) {
            Some(missing) => format!(
                "plugin `{}` requires `{missing}`, which the project doesn't have, so it is not loaded",
                manifest.name
            ),
            None => format!(
                "plugin `{}` requires itself through other plugins, so it is not loaded",
                manifest.name
            ),
        };
        problems.push(Problem { file: manifest.declared_in.clone(), message });
    }
    (manifests, problems)
}

/// Entries under `plugins` that are not a plugin declaration. They are also
/// not the settings of a plugin of the project. Such an entry is almost
/// always a mistake, because nothing will read it.
pub fn unclaimed<'a>(project: &'a Project, manifests: &[Manifest]) -> Vec<&'a PluginEntry> {
    project
        .plugins
        .iter()
        .filter(|entry| {
            !is_declaration(entry)
                && !manifests.iter().any(|manifest| {
                    manifest.settings.as_deref().is_some_and(|settings| {
                        entry.extends(manifest.module_root.as_deref(), settings)
                    })
                })
        })
        .collect()
}

/// True if `name` reaches itself through its requirements.
fn requires_itself(manifests: &[Manifest], name: &str) -> bool {
    let mut stack: Vec<&str> = manifests
        .iter()
        .find(|manifest| manifest.name == name)
        .map(|manifest| manifest.requires.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let mut seen: Vec<&str> = Vec::new();
    while let Some(next) = stack.pop() {
        if next == name {
            return true;
        }
        if seen.contains(&next) {
            continue;
        }
        seen.push(next);
        if let Some(manifest) = manifests.iter().find(|manifest| manifest.name == next) {
            stack.extend(manifest.requires.iter().map(String::as_str));
        }
    }
    false
}

/// The plugins that `main` must load. The requirements come first and
/// `main` comes last.
pub fn load_order<'a>(manifests: &'a [Manifest], main: &str) -> Result<Vec<&'a Manifest>, String> {
    fn visit<'a>(
        manifests: &'a [Manifest],
        name: &str,
        order: &mut Vec<&'a Manifest>,
        path: &mut Vec<String>,
    ) -> Result<(), String> {
        if order.iter().any(|manifest| manifest.name == name) {
            return Ok(());
        }
        if path.iter().any(|seen| seen == name) {
            return Err(format!("plugin `{name}` requires itself through {}", path.join(", ")));
        }
        let manifest = manifests
            .iter()
            .find(|manifest| manifest.name == name)
            .ok_or_else(|| format!("no plugin named `{name}` is loaded"))?;
        path.push(name.to_string());
        for required in &manifest.requires {
            visit(manifests, required, order, path)?;
        }
        path.pop();
        order.push(manifest);
        Ok(())
    }
    let mut order = Vec::new();
    visit(manifests, main, &mut order, &mut Vec::new())?;
    Ok(order)
}

/// The plugins that `piton build` runs. Each has a build, and the project
/// turned it on. A plugin that the project declares is on. A bundled plugin
/// is on when the project lists its settings. Thus a project that does not
/// name a bundled plugin gets none of its output.
pub fn builders<'a>(project: &Project, manifests: &'a [Manifest]) -> Vec<&'a Manifest> {
    manifests
        .iter()
        .filter(|manifest| manifest.build)
        .filter(|manifest| {
            !manifest.bundled
                || manifest.settings.as_deref().is_some_and(|settings| {
                    project
                        .plugins
                        .iter()
                        .any(|entry| !is_declaration(entry) && entry.extends(manifest.module_root.as_deref(), settings))
                })
        })
        .collect()
}

/// True if a configuration entry is a plugin declaration and not settings.
pub fn is_declaration(entry: &PluginEntry) -> bool {
    entry.extends(Some(prelude::PLUGIN_PACKAGE), PLUGIN_ANCHOR)
}

/// Reads the declaration of a bundled plugin. It compiles the embedded
/// module of the declaration.
fn bundled_manifest(plugin: &bundled::Bundled) -> Result<Manifest, String> {
    let compilation = Compilation::build(Project::for_file(Path::new(plugin.declaration)));
    if compilation.has_errors() {
        let first = compilation
            .diagnostics
            .iter()
            .next()
            .map(|diagnostic| diagnostic.message.clone())
            .unwrap_or_default();
        return Err(format!("the bundled declaration does not compile: {first}"));
    }
    let entry = compilation.resolution.entry;
    let anchor = compilation
        .resolution
        .exported_names(entry)
        .into_iter()
        .find_map(|name| {
            match compilation.resolution.lookup_export(entry, &name, &mut Default::default()) {
                Some(Symbol::Anchor(anchor))
                    if compilation.store().anchor(anchor).keyword == PLUGIN_KEYWORD =>
                {
                    Some(anchor)
                }
                _ => None,
            }
        })
        .ok_or_else(|| "the bundled declaration exports no piton-plugin".to_string())?;
    let properties = piton_compile::json::properties_map(
        &compilation.store().anchor(anchor).properties,
        &compilation,
    );
    read(
        &properties,
        Source::Bundled(plugin.files),
        Some(plugin.module_root.to_string()),
        Path::new(plugin.declaration),
        true,
    )
}

fn text(properties: &Map<String, Json>, key: &str) -> Option<String> {
    properties
        .get(key)
        .and_then(Json::as_str)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn strings(value: Option<&Json>, what: &str) -> Result<Vec<String>, String> {
    match value {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(|text| text.trim().to_string())
                    .ok_or_else(|| format!("{what} has to be a list of names"))
            })
            .collect(),
        Some(_) => Err(format!("{what} has to be a list of names")),
    }
}

/// The name of a plugin. It is lowercase words with hyphens between them.
/// The name is also part of a package path.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|part| {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// Reads the properties of a declaration.
fn read(
    properties: &Map<String, Json>,
    source: Source,
    module_root: Option<String>,
    declared_in: &Path,
    bundled: bool,
) -> Result<Manifest, String> {
    let name = text(properties, "name").ok_or("it has no name")?;
    if !valid_name(&name) {
        return Err(format!("`{name}` is not a plugin name; use lowercase words joined by hyphens"));
    }
    let entry = text(properties, "entry").ok_or("it has no entry")?;
    let entry = entry.trim_start_matches("./").to_string();
    if entry.starts_with('/') || entry.split('/').any(|part| part == "..") {
        return Err(format!("its entry `{entry}` has to be inside the plugin's directory"));
    }
    if source.read(&entry).is_none() {
        return Err(format!("its entry `{entry}` does not exist"));
    }

    let capabilities = match properties.get("capabilities") {
        None | Some(Json::Null) => Capabilities::default(),
        Some(Json::Object(map)) => {
            let mut capabilities = Capabilities::default();
            for (key, value) in map {
                match key.as_str() {
                    "process" => capabilities.process = strings(Some(value), "capabilities.process")?,
                    "files" => match value {
                        Json::Null => {}
                        Json::String(level) if level == "read" => capabilities.read_files = true,
                        Json::String(level) if level == "none" => {}
                        _ => return Err("capabilities.files is read or none".to_string()),
                    },
                    "temporaryFiles" => {
                        capabilities.temporary_files = value
                            .as_bool()
                            .ok_or("capabilities.temporaryFiles is true or false")?
                    }
                    "interactive" => {
                        capabilities.interactive =
                            value.as_bool().ok_or("capabilities.interactive is true or false")?
                    }
                    "scaffold" => {
                        capabilities.scaffold =
                            value.as_bool().ok_or("capabilities.scaffold is true or false")?
                    }
                    "outputs" | "edits" => {
                        let patterns = strings(Some(value), &format!("capabilities.{key}"))?;
                        for pattern in &patterns {
                            if pattern.starts_with('"') || pattern.starts_with('\'') {
                                return Err(format!(
                                    "capabilities.{key} `{pattern}` is quoted, and Piton keeps quotes as part of the text; write it without them"
                                ));
                            }
                            if pattern.starts_with('/') || pattern.split('/').any(|part| part == "..") {
                                return Err(format!("capabilities.{key} `{pattern}` has to stay inside the project"));
                            }
                            globset::Glob::new(pattern)
                                .map_err(|error| format!("capabilities.{key} `{pattern}` is not a glob: {error}"))?;
                        }
                        if key == "outputs" {
                            capabilities.outputs = patterns;
                        } else {
                            capabilities.edits = patterns;
                        }
                    }
                    other => {
                        return Err(format!(
                            "`{other}` is not a capability; they are process, files, temporaryFiles, outputs, edits, interactive and scaffold"
                        ))
                    }
                }
            }
            capabilities
        }
        Some(_) => return Err("capabilities has to be a dictionary".to_string()),
    };

    let mut commands = Vec::new();
    match properties.get("commands") {
        None | Some(Json::Null) => {}
        Some(Json::Object(map)) => {
            for (command_name, spec) in map {
                commands.push(read_command(command_name, spec)?);
            }
        }
        Some(_) => return Err("commands has to be a dictionary of commands".to_string()),
    }

    let requires = strings(properties.get("requires"), "requires")?;
    if requires.contains(&name) {
        return Err("it requires itself".to_string());
    }
    let placeholders = strings(properties.get("placeholders"), "placeholders")?;
    let flag = |key: &str| -> Result<bool, String> {
        match properties.get(key) {
            None | Some(Json::Null) => Ok(false),
            Some(Json::Bool(value)) => Ok(*value),
            Some(_) => Err(format!("{key} is true or false")),
        }
    };
    let build = flag("build")?;
    let inspect = flag("inspect")?;
    let reach = flag("reach")?;

    Ok(Manifest {
        name,
        description: text(properties, "description"),
        entry,
        source,
        module_root,
        settings: text(properties, "settings"),
        commands,
        capabilities,
        declared_in: declared_in.to_path_buf(),
        bundled,
        requires,
        placeholders,
        build,
        inspect,
        reach,
    })
}

fn read_command(name: &str, spec: &Json) -> Result<CommandSpec, String> {
    if !valid_name(name) {
        return Err(format!("`{name}` is not a command name; use lowercase words joined by hyphens"));
    }
    let Some(spec) = spec.as_object() else {
        return Err(format!("command `{name}` has to be a dictionary"));
    };
    let mut arguments = Vec::new();
    match spec.get("arguments") {
        None | Some(Json::Null) => {}
        Some(Json::Object(map)) => {
            for (argument, value) in map {
                arguments.push(read_argument(name, argument, value)?);
            }
        }
        Some(_) => return Err(format!("command `{name}`: arguments has to be a dictionary")),
    }
    for argument in &arguments {
        for other in &argument.conflicts {
            if !arguments.iter().any(|candidate| &candidate.name == other) {
                return Err(format!(
                    "command `{name}`: `{}` conflicts with `{other}`, which it doesn't have",
                    argument.name
                ));
            }
        }
    }
    let group = text(spec, "group");
    if let Some(group) = &group {
        if !valid_name(group) {
            return Err(format!("command `{name}`: group `{group}` is not a name; use lowercase words joined by hyphens"));
        }
    }
    let standalone = match spec.get("standalone") {
        None | Some(Json::Null) => false,
        Some(Json::Bool(value)) => *value,
        Some(_) => return Err(format!("command `{name}`: standalone is true or false")),
    };
    Ok(CommandSpec {
        name: name.to_string(),
        group,
        about: text(spec, "about"),
        help: text(spec, "help"),
        arguments,
        standalone,
    })
}

fn read_argument(command: &str, name: &str, value: &Json) -> Result<ArgSpec, String> {
    let what = format!("command `{command}`, argument `{name}`");
    let Some(spec) = value.as_object() else {
        return Err(format!("{what} has to be a dictionary"));
    };
    for key in spec.keys() {
        if !["help", "positional", "multiple", "type", "choices", "short", "minimum", "conflicts", "default", "trailing", "optionalValue"]
            .contains(&key.as_str())
        {
            return Err(format!("{what}: `{key}` is not an argument setting"));
        }
    }
    let flag = |key: &str| -> Result<bool, String> {
        match spec.get(key) {
            None | Some(Json::Null) => Ok(false),
            Some(Json::Bool(value)) => Ok(*value),
            Some(_) => Err(format!("{what}: {key} is true or false")),
        }
    };
    let kind = match text(spec, "type").as_deref() {
        None | Some("string") => ArgKind::String,
        Some("number") => ArgKind::Number,
        Some("flag") => ArgKind::Flag,
        Some(other) => return Err(format!("{what}: type `{other}` is not flag, string or number")),
    };
    let short = match text(spec, "short") {
        None => None,
        Some(short) if short.chars().count() == 1 => short.chars().next(),
        Some(short) => return Err(format!("{what}: short `{short}` has to be one letter")),
    };
    let minimum = match spec.get("minimum") {
        None | Some(Json::Null) => None,
        Some(value) => Some(value.as_f64().ok_or_else(|| format!("{what}: minimum is a number"))?),
    };
    let positional = flag("positional")?;
    if positional && (kind == ArgKind::Flag || short.is_some()) {
        return Err(format!("{what}: a positional argument has no type flag and no short name"));
    }
    Ok(ArgSpec {
        name: name.to_string(),
        help: text(spec, "help"),
        positional,
        multiple: flag("multiple")?,
        kind,
        choices: strings(spec.get("choices"), &format!("{what}: choices"))?,
        short,
        minimum,
        conflicts: strings(spec.get("conflicts"), &format!("{what}: conflicts"))?,
        default: match spec.get("default") {
            None | Some(Json::Null) => None,
            Some(Json::String(text)) => Some(text.clone()),
            Some(Json::Number(number)) => Some(number.to_string()),
            Some(_) => return Err(format!("{what}: default is text or a number")),
        },
        trailing: {
            let trailing = flag("trailing")?;
            if trailing && !(positional && flag("multiple")?) {
                return Err(format!("{what}: only a positional, multiple argument can be trailing"));
            }
            trailing
        },
        optional_value: {
            let optional = flag("optionalValue")?;
            if optional && (positional || kind == ArgKind::Flag || flag("multiple")?) {
                return Err(format!("{what}: only a single option that takes a value can have an optional value"));
            }
            optional
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_test_plugin_is_declared() {
        let (manifests, problems) = discover(&Project::for_file(Path::new("/nonexistent/x.pi")));
        assert!(problems.is_empty(), "{problems:?}");
        let test = manifests.iter().find(|manifest| manifest.name == "test").expect("test plugin");
        assert!(test.bundled);
        assert_eq!(test.entry, "src/index.ts");
        assert_eq!(test.settings.as_deref(), Some("TestConfig"));
        assert_eq!(test.module_root.as_deref(), Some("@piton/test"));
        assert!(test.capabilities.process.contains(&"claude".to_string()));
        assert!(test.capabilities.temporary_files);
        let command = &test.commands[0];
        assert_eq!(command.name, "test");
        let serial = command.arguments.iter().find(|argument| argument.name == "serial").expect("serial");
        assert_eq!(serial.kind, ArgKind::Flag);
        assert_eq!(serial.conflicts, ["jobs"]);
        let targets = &command.arguments[0];
        assert!(targets.positional && targets.multiple);
    }
}
