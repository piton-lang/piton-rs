//! `piton init` — start a project from one of the templates.
//!
//! The templates are the directories in `create-templates/`, compiled into
//! the binary by `build.rs`, so `init` works offline and a release always
//! writes the templates it shipped with. They come in groups, like `piton` and
//! `belay`, and a template is named with its group: `belay/application`.
//!
//! A template whose config lists Belay adapters also asks which adapters to
//! build for. The adapters it lists are the default, and the chosen ones
//! replace them in the written config. Files a template keeps under
//! `.adapters/<target id>/` are written only when that adapter is chosen.
//!
//! Nothing is ever overwritten: when any file a template would write already
//! exists, the command says which and writes none of them.

use std::borrow::Cow;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use inquire::error::InquireResult;
use inquire::list_option::ListOption;
use inquire::validator::Validation;
use inquire::{InquireError, MultiSelect, Select, Text};
use piton_belay::adapter::Adapter;

use anstream::{eprintln, println};

use crate::style::{self, paint};
use crate::{report, EXIT_ERRORS, EXIT_SUCCESS};

/// The file in a template that holds its project configuration.
const CONFIG: &str = "piton.config.pi";

/// The directory in a template holding, for each adapter by target id, files
/// written only when that adapter is chosen.
const ADAPTER_FILES: &str = ".adapters/";

/// A group of templates, like `belay`.
pub struct Group {
    pub name: &'static str,
    pub description: &'static str,
    pub templates: &'static [Template],
}

/// One project template: what it writes, relative to the new project. Its
/// name includes its group, like `belay/application`.
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));

pub fn run(
    directory: Option<&Path>,
    template: Option<&str>,
    adapters: &[String],
    list: bool,
) -> u8 {
    if list {
        let width = GROUPS
            .iter()
            .flat_map(|group| group.templates)
            .map(|template| template.name.len())
            .max()
            .unwrap_or(0);
        for group in GROUPS {
            println!(
                "{}  {}",
                paint(style::HEADING, group.name),
                paint(style::DIM, group.description)
            );
            for template in group.templates {
                println!(
                    "  {}  {}",
                    paint(style::NAME, format!("{:<width$}", template.name)),
                    template.description
                );
            }
        }
        return EXIT_SUCCESS;
    }

    let directory = match directory {
        Some(directory) => directory.to_path_buf(),
        None if std::io::stdin().is_terminal() => match ask_name() {
            Some(directory) => directory,
            None => return EXIT_ERRORS,
        },
        None => PathBuf::from("."),
    };

    let template = match template {
        Some(name) => match find(name) {
            Some(Found::Template(template)) => template,
            Some(Found::Group(group)) if std::io::stdin().is_terminal() => match pick(group) {
                Some(template) => template,
                None => return EXIT_ERRORS,
            },
            Some(Found::Group(group)) => {
                report::fail(format!(
                    "`{name}` is a group of templates; pick one with --template: {}",
                    names(group.templates)
                ));
                return EXIT_ERRORS;
            }
            None => {
                report::fail(format!(
                    "`{name}` is not a template; the templates are {}",
                    names(GROUPS.iter().flat_map(|group| group.templates))
                ));
                return EXIT_ERRORS;
            }
        },
        None if std::io::stdin().is_terminal() => match pick_group().and_then(pick) {
            Some(template) => template,
            None => return EXIT_ERRORS,
        },
        None => {
            report::fail(format!(
                "pick a template with --template: {}",
                names(GROUPS.iter().flat_map(|group| group.templates))
            ));
            report::help("`piton init --list` describes each one");
            return EXIT_ERRORS;
        }
    };

    let config = template
        .files
        .iter()
        .find(|(path, _)| *path == CONFIG)
        .map(|(_, contents)| String::from_utf8_lossy(contents).into_owned());
    let listed = config.as_deref().map(listed_adapters).unwrap_or_default();
    let chosen = if listed.is_empty() {
        if !adapters.is_empty() {
            report::fail(format!(
                "the `{}` template doesn't use Belay, so it has no adapters to choose",
                template.name
            ));
            return EXIT_ERRORS;
        }
        Vec::new()
    } else if !adapters.is_empty() {
        match adapters_named(adapters.iter().map(String::as_str)) {
            Some(chosen) => chosen,
            None => return EXIT_ERRORS,
        }
    } else if std::io::stdin().is_terminal() {
        match pick_adapters(&listed) {
            Some(chosen) => chosen,
            None => return EXIT_ERRORS,
        }
    } else {
        listed.clone()
    };

    let files: Vec<(&str, Cow<[u8]>)> = template
        .files
        .iter()
        .filter_map(|(path, contents)| {
            let Some(rest) = path.strip_prefix(ADAPTER_FILES) else {
                return Some((*path, Cow::Borrowed(*contents)));
            };
            // `.adapters/<target id>/<path>` is written at `<path>`, and only
            // when that adapter is chosen.
            let (target, path) = rest.split_once('/')?;
            chosen
                .iter()
                .any(|adapter| adapter.target_id == target)
                .then_some((path, Cow::Borrowed(*contents)))
        })
        .map(|(path, contents)| match &config {
            Some(config) if path == CONFIG && !chosen.is_empty() => (
                path,
                Cow::Owned(with_adapters(config, &chosen).into_bytes()),
            ),
            _ => (path, contents),
        })
        .collect();

    let existing: Vec<&str> = files
        .iter()
        .map(|(path, _)| *path)
        .filter(|path| directory.join(path).exists())
        .collect();
    if !existing.is_empty() {
        report::fail(format!(
            "`{}` already has {} the `{}` template would write, so nothing was written",
            directory.display(),
            report::plural(existing.len(), "a file", "files"),
            template.name
        ));
        for path in existing {
            report::note(format_args!("{}", display(&directory, path)));
        }
        return EXIT_ERRORS;
    }

    for (path, contents) in &files {
        let destination = directory.join(path);
        if let Some(parent) = destination.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                report::fail(format!("cannot create `{}`: {error}", parent.display()));
                return EXIT_ERRORS;
            }
        }
        if let Err(error) = std::fs::write(&destination, contents.as_ref()) {
            report::fail(format!("cannot write `{}`: {error}", destination.display()));
            return EXIT_ERRORS;
        }
        println!("{}", report::added(display(&directory, path)));
    }

    let mut summary = format!(
        "Created {} in {}",
        paint(style::NAME, template.name),
        paint(style::NAME, directory.display())
    );
    if !chosen.is_empty() {
        let targets: Vec<String> = chosen
            .iter()
            .map(|adapter| adapter.target_id.to_string())
            .collect();
        summary.push_str(&format!(" for {}", targets.join(", ")));
    }
    report::done(format!(
        "{summary} {}",
        paint(
            style::DIM,
            format!("({} {})", files.len(), report::plural(files.len(), "file", "files"))
        )
    ));
    eprintln!();
    eprintln!("{}", paint(style::HEADING, "Next"));
    if directory != Path::new(".") {
        eprintln!(
            "  {}",
            paint(style::NAME, format!("cd {}", directory.display()))
        );
    }
    eprintln!("  {}", paint(style::NAME, "piton build"));
    EXIT_SUCCESS
}

/// Asks for the project's name, which is the directory it is created in. An
/// empty answer creates it in the current directory.
fn ask_name() -> Option<PathBuf> {
    let answer = answered(
        Text::new("Project name:")
            .with_help_message("the directory to create it in; leave empty for the current one")
            .prompt(),
    )?;
    let answer = answer.trim();
    Some(PathBuf::from(if answer.is_empty() { "." } else { answer }))
}

/// What `--template` names: a whole group, or one template in it.
enum Found {
    Group(&'static Group),
    Template(&'static Template),
}

fn find(name: &str) -> Option<Found> {
    GROUPS.iter().find_map(|group| {
        if group.name == name {
            return Some(Found::Group(group));
        }
        group
            .templates
            .iter()
            .find(|template| template.name == name)
            .map(Found::Template)
    })
}

/// A group or template as the picker shows it: its name, padded to line up
/// with the others offered, then its description.
struct Choice<T> {
    value: T,
    name: &'static str,
    description: &'static str,
    width: usize,
}

impl<T> std::fmt::Display for Choice<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:<width$}  {}",
            self.name,
            self.description,
            width = self.width
        )
    }
}

/// Offers `choices`, and returns the one picked. A single choice is taken
/// without asking.
fn choose<T>(message: &str, mut choices: Vec<Choice<T>>) -> Option<T> {
    let width = choices
        .iter()
        .map(|choice| choice.name.len())
        .max()
        .unwrap_or(0);
    for choice in &mut choices {
        choice.width = width;
    }
    if choices.len() == 1 {
        return choices.pop().map(|choice| choice.value);
    }
    let chosen = answered(
        Select::new(message, choices)
            .with_formatter(&|choice| choice.value.name.to_string())
            .prompt(),
    )?;
    Some(chosen.value)
}

/// Asks which group of templates to start from.
fn pick_group() -> Option<&'static Group> {
    let choices = GROUPS
        .iter()
        .map(|group| Choice {
            value: group,
            name: group.name,
            description: group.description,
            width: 0,
        })
        .collect();
    choose("Kind of project:", choices)
}

/// Asks which template in `group` to use.
fn pick(group: &'static Group) -> Option<&'static Template> {
    let choices = group
        .templates
        .iter()
        .map(|template| Choice {
            value: template,
            name: template.name.rsplit('/').next().unwrap_or(template.name),
            description: template.description,
            width: 0,
        })
        .collect();
    choose("Template:", choices)
}

/// An adapter as the picker shows it.
struct AdapterChoice(&'static Adapter);

impl std::fmt::Display for AdapterChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let width = Adapter::all()
            .iter()
            .map(|adapter| adapter.platform.len())
            .max()
            .unwrap_or(0);
        write!(f, "{:<width$}  {}", self.0.platform, self.0.target_id)
    }
}

/// Asks which adapters to build for, with the ones the template lists
/// already checked. At least one has to be chosen.
fn pick_adapters(listed: &[&'static Adapter]) -> Option<Vec<&'static Adapter>> {
    let defaults: Vec<usize> = Adapter::all()
        .iter()
        .enumerate()
        .filter(|(_, adapter)| {
            listed
                .iter()
                .any(|listed| listed.target_id == adapter.target_id)
        })
        .map(|(index, _)| index)
        .collect();
    let choices = Adapter::all().iter().map(AdapterChoice).collect();
    let chosen = answered(
        MultiSelect::new("Adapters:", choices)
            .with_default(&defaults)
            .with_help_message("space to toggle, enter to accept")
            .with_formatter(&|chosen| {
                chosen
                    .iter()
                    .map(|choice| choice.value.0.target_id)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .with_validator(|chosen: &[ListOption<&AdapterChoice>]| {
                Ok(if chosen.is_empty() {
                    Validation::Invalid("choose at least one adapter".into())
                } else {
                    Validation::Valid
                })
            })
            .prompt(),
    )?;
    Some(chosen.into_iter().map(|choice| choice.0).collect())
}

/// The adapters `--adapter` names by target id, in the order `Adapter::all`
/// gives, each once.
fn adapters_named<'a>(names: impl Iterator<Item = &'a str>) -> Option<Vec<&'static Adapter>> {
    let mut chosen: Vec<&'static Adapter> = Vec::new();
    for name in names {
        let name = name.trim();
        let Some(adapter) = Adapter::by_target(name) else {
            let known = Adapter::all()
                .iter()
                .map(|adapter| format!("`{}`", adapter.target_id))
                .collect::<Vec<_>>()
                .join(", ");
            report::fail(format!(
                "`{name}` is not an adapter; the adapters are {known}"
            ));
            return None;
        };
        if !chosen
            .iter()
            .any(|seen| seen.target_id == adapter.target_id)
        {
            chosen.push(adapter);
        }
    }
    chosen.sort_by_key(|adapter| {
        Adapter::all()
            .iter()
            .position(|known| known.target_id == adapter.target_id)
    });
    Some(chosen)
}

/// The answer to a prompt, or None once the failure is reported. Escape and
/// Ctrl+C stop init before anything is written.
fn answered<T>(answer: InquireResult<T>) -> Option<T> {
    match answer {
        Ok(answer) => Some(answer),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            report::fail("init was cancelled, so nothing was written");
            None
        }
        Err(error) => {
            report::fail(format!("cannot ask: {error}"));
            None
        }
    }
}

/// The adapters a template's config lists as `- {ClaudeCodeAdapter}` lines.
/// Empty when the template doesn't use Belay.
fn listed_adapters(config: &str) -> Vec<&'static Adapter> {
    config.lines().filter_map(adapter_item).collect()
}

/// The adapter a `- {ExportName}` list item names, if it names one.
fn adapter_item(line: &str) -> Option<&'static Adapter> {
    let name = line.trim().strip_prefix("- {")?.strip_suffix('}')?;
    Adapter::all()
        .iter()
        .find(|adapter| adapter.export_name == name)
}

/// The template's config with its adapters replaced by `chosen`: in the
/// `@piton/belay` import and in the `adapters` list. The template writes the
/// import on one line; formatting afterwards wraps it when it grows.
fn with_adapters(config: &str, chosen: &[&'static Adapter]) -> String {
    const IMPORT: &str = "from @piton/belay import ";
    let mut out = String::new();
    let mut listed = false;
    for line in config.lines() {
        if let Some(names) = line.strip_prefix(IMPORT) {
            let names: Vec<&str> = names
                .split(',')
                .map(str::trim)
                .filter(|name| {
                    !Adapter::all()
                        .iter()
                        .any(|adapter| adapter.export_name == *name)
                })
                .chain(chosen.iter().map(|adapter| adapter.export_name))
                .collect();
            out.push_str(IMPORT);
            out.push_str(&names.join(", "));
            out.push('\n');
        } else if adapter_item(line).is_some() {
            if !listed {
                let indent = &line[..line.len() - line.trim_start().len()];
                for adapter in chosen {
                    out.push_str(&format!("{indent}- {{{}}}\n", adapter.export_name));
                }
                listed = true;
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    piton_syntax::format::format(&out, Path::new(CONFIG))
}

fn names(templates: impl IntoIterator<Item = &'static Template>) -> String {
    templates
        .into_iter()
        .map(|template| format!("`{}`", template.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A written path as the person running `init` would type it.
fn display(directory: &Path, path: &str) -> String {
    if directory == Path::new(".") {
        path.to_string()
    } else {
        directory.join(path).display().to_string()
    }
}
