//! What a plugin declares about itself.
//!
//! A plugin's declaration is a `piton-plugin` anchor, so everything that can
//! be known without running it is Piton: its name, the commands it adds and
//! their arguments, what it may do on the machine, and where its code is. The
//! CLI builds its help and parses arguments from the declaration alone, and
//! only runs JavaScript when one of the plugin's commands is invoked.

use std::path::{Path, PathBuf};

use piton_compile::config::PluginEntry;
use piton_compile::{prelude, Compilation, Project, Symbol};
use serde_json::{Map, Value as Json};

use crate::bundled;

/// The keyword a plugin is declared with.
pub const PLUGIN_KEYWORD: &str = "piton-plugin";
/// The abstract anchor every declaration implements.
pub const PLUGIN_ANCHOR: &str = "PitonPlugin";

/// Where a plugin's code is.
#[derive(Debug, Clone)]
pub enum Source {
    /// Built into the compiler: path inside the plugin, and its text.
    Bundled(&'static [(&'static str, &'static str)]),
    /// On disk, in the directory its declaration is in. Nothing outside it
    /// is loaded.
    Directory(PathBuf),
}

impl Source {
    /// The text of a file inside the plugin, by its path relative to the
    /// plugin, like `src/index.ts`.
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

/// What a plugin may do on the machine. Everything not listed is refused.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capabilities {
    /// Programs it may run, by name, looked up on PATH.
    pub process: Vec<String>,
    /// Whether it may read files inside the project.
    pub read_files: bool,
    /// Whether it may write temporary files, removed when its command ends.
    pub temporary_files: bool,
}

/// How an argument is given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArgKind {
    /// Present or not.
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
}

/// One command a plugin adds to the CLI.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandSpec {
    pub name: String,
    pub about: Option<String>,
    pub help: Option<String>,
    pub arguments: Vec<ArgSpec>,
}

/// A plugin, as its declaration describes it.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub name: String,
    pub description: Option<String>,
    /// The module its behaviour starts from, relative to the plugin.
    pub entry: String,
    pub source: Source,
    /// The package its Piton files are imported from, for a bundled plugin.
    pub module_root: Option<String>,
    /// The abstract anchor a project's settings for it implement.
    pub settings: Option<String>,
    pub commands: Vec<CommandSpec>,
    pub capabilities: Capabilities,
    /// The file it is declared in, for diagnostics.
    pub declared_in: PathBuf,
    pub bundled: bool,
}

/// What is wrong with a declaration, with the file it is in.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub file: PathBuf,
    pub message: String,
}

/// Every plugin a project has: the bundled ones, then those its
/// configuration declares, in the order listed. A declaration that isn't
/// well-formed is left out and reported.
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

    // Two plugins can't both add a command, or which one runs would depend
    // on the order they were listed in: the later one's is reported and left
    // out.
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
    (manifests, problems)
}

/// Entries under `plugins` that are neither a plugin's declaration nor the
/// settings of a plugin the project has: most likely a mistake, since nothing
/// will read them.
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

/// Whether a configuration entry is a plugin declaration rather than settings.
pub fn is_declaration(entry: &PluginEntry) -> bool {
    entry.extends(Some(prelude::PLUGIN_PACKAGE), PLUGIN_ANCHOR)
}

/// Reads a bundled plugin's declaration, compiled from the module it is
/// embedded as.
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

/// A plugin's name: lowercase words joined by hyphens, since it is also part
/// of a package path.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|part| {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// Reads a declaration's properties.
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
                    other => {
                        return Err(format!(
                            "`{other}` is not a capability; they are process, files and temporaryFiles"
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
    Ok(CommandSpec {
        name: name.to_string(),
        about: text(spec, "about"),
        help: text(spec, "help"),
        arguments,
    })
}

fn read_argument(command: &str, name: &str, value: &Json) -> Result<ArgSpec, String> {
    let what = format!("command `{command}`, argument `{name}`");
    let Some(spec) = value.as_object() else {
        return Err(format!("{what} has to be a dictionary"));
    };
    for key in spec.keys() {
        if !["help", "positional", "multiple", "type", "choices", "short", "minimum", "conflicts"]
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
