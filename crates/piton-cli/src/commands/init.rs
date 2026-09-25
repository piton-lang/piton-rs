//! `piton init` — start a project from one of the templates.
//!
//! The templates are the directories in `create-templates/`, compiled into
//! the binary by `build.rs`, so `init` works offline and a release always
//! writes the templates it shipped with.
//!
//! Nothing is ever overwritten: when any file a template would write already
//! exists, the command says which and writes none of them.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use crate::{report, EXIT_ERRORS, EXIT_SUCCESS};

/// One project template: what it writes, relative to the new project.
pub struct Template {
    pub name: &'static str,
    pub description: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

include!(concat!(env!("OUT_DIR"), "/templates.rs"));

pub fn run(directory: Option<&Path>, template: Option<&str>, list: bool) -> u8 {
    if list {
        for template in TEMPLATES {
            println!(
                "{:<width$}  {}",
                template.name,
                template.description,
                width = name_width()
            );
        }
        return EXIT_SUCCESS;
    }

    let template = match template {
        Some(name) => match TEMPLATES.iter().find(|template| template.name == name) {
            Some(template) => template,
            None => {
                report::fail(format!(
                    "`{name}` is not a template; the templates are {}",
                    names()
                ));
                return EXIT_ERRORS;
            }
        },
        None if std::io::stdin().is_terminal() => match pick() {
            Some(template) => template,
            None => return EXIT_ERRORS,
        },
        None => {
            report::fail(format!("pick a template with --template: {}", names()));
            eprintln!("  help: `piton init --list` describes each one");
            return EXIT_ERRORS;
        }
    };

    let directory = directory
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let existing: Vec<&str> = template
        .files
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
            eprintln!("  note: {}", display(&directory, path));
        }
        return EXIT_ERRORS;
    }

    for (path, contents) in template.files {
        let destination = directory.join(path);
        if let Some(parent) = destination.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                report::fail(format!("cannot create `{}`: {error}", parent.display()));
                return EXIT_ERRORS;
            }
        }
        if let Err(error) = std::fs::write(&destination, contents) {
            report::fail(format!("cannot write `{}`: {error}", destination.display()));
            return EXIT_ERRORS;
        }
        println!("{}", display(&directory, path));
    }

    eprintln!(
        "created a `{}` project with {} {}",
        template.name,
        template.files.len(),
        report::plural(template.files.len(), "file", "files")
    );
    if directory != Path::new(".") {
        eprintln!("  next: cd {} && piton build", directory.display());
    } else {
        eprintln!("  next: piton build");
    }
    EXIT_SUCCESS
}

/// Asks which template to use, by number or name.
fn pick() -> Option<&'static Template> {
    let width = name_width();
    eprintln!("Which template?");
    for (index, template) in TEMPLATES.iter().enumerate() {
        eprintln!(
            "  {}. {:<width$}  {}",
            index + 1,
            template.name,
            template.description
        );
    }
    eprint!("> ");
    let _ = std::io::stderr().flush();

    let mut answer = String::new();
    if std::io::stdin().lock().read_line(&mut answer).is_err() {
        report::fail("cannot read the answer");
        return None;
    }
    let answer = answer.trim();
    let chosen = match answer.parse::<usize>() {
        Ok(number) => number.checked_sub(1).and_then(|index| TEMPLATES.get(index)),
        Err(_) => TEMPLATES.iter().find(|template| template.name == answer),
    };
    if chosen.is_none() {
        report::fail(format!("`{answer}` is not one of the templates"));
    }
    chosen
}

fn names() -> String {
    TEMPLATES
        .iter()
        .map(|template| format!("`{}`", template.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn name_width() -> usize {
    TEMPLATES
        .iter()
        .map(|template| template.name.len())
        .max()
        .unwrap_or(0)
}

/// A written path as the person running `init` would type it.
fn display(directory: &Path, path: &str) -> String {
    if directory == Path::new(".") {
        path.to_string()
    } else {
        directory.join(path).display().to_string()
    }
}
