//! Compiles the project templates `piton init` offers into the binary.
//!
//! Every directory in `create-templates/` at the repository root is a group
//! of templates, like `belay`, and every directory in a group is one template,
//! like `belay/application`: its files are what `piton init` writes. Groups and
//! templates each have a `.template` file holding the one-line description the
//! picker shows. Adding a template is adding a directory; nothing here lists
//! them by name.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The file in each template that describes it, and is never written out.
const DESCRIPTION: &str = ".template";

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.join("../../create-templates");
    // A directory is scanned in full, so an edited, added, or removed
    // template file rebuilds the table.
    println!("cargo:rerun-if-changed={}", root.display());

    let mut out = String::from("pub static GROUPS: &[Group] = &[\n");
    for group in directories(&root) {
        let name = group.file_name().expect("name").to_string_lossy();
        writeln!(
            out,
            "    Group {{\n        name: {name:?},\n        description: {:?},\n        templates: &[",
            description(&group, &name)
        )
        .unwrap();
        let templates = directories(&group);
        assert!(!templates.is_empty(), "create-templates/{name} has no templates");
        for template in &templates {
            let template_name = template.file_name().expect("name").to_string_lossy();
            let path = format!("{name}/{template_name}");

            let mut files = Vec::new();
            collect(template, template, &mut files);
            files.sort();
            assert!(!files.is_empty(), "create-templates/{path} has no files");

            writeln!(
                out,
                "            Template {{\n                name: {path:?},\n                description: {:?},\n                files: &[",
                description(template, &path)
            )
            .unwrap();
            for relative in &files {
                let absolute = template.join(relative);
                writeln!(
                    out,
                    "                    ({relative:?}, include_bytes!({:?})),",
                    absolute
                        .canonicalize()
                        .expect("template file")
                        .display()
                        .to_string()
                )
                .unwrap();
            }
            out.push_str("                ],\n            },\n");
        }
        out.push_str("        ],\n    },\n");
    }
    out.push_str("];\n");

    let destination =
        PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("templates.rs");
    std::fs::write(destination, out).expect("write templates.rs");
}

/// The directories directly inside `directory`, sorted.
fn directories(directory: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()))
        .map(|entry| entry.expect("template entry").path())
        .filter(|path| path.is_dir())
        .collect();
    found.sort();
    found
}

/// The first line of the `.template` file in `directory`, which has to exist
/// and say something.
fn description(directory: &Path, name: &str) -> String {
    let description = std::fs::read_to_string(directory.join(DESCRIPTION))
        .unwrap_or_else(|_| panic!("create-templates/{name} has no {DESCRIPTION} file"));
    let description = description.lines().next().unwrap_or_default().trim();
    assert!(!description.is_empty(), "create-templates/{name}/{DESCRIPTION} is empty");
    description.to_string()
}

/// Every file under `directory` but the description, as `/`-separated paths
/// relative to `template`.
fn collect(template: &Path, directory: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).expect("template directory") {
        let path = entry.expect("template entry").path();
        if path.is_dir() {
            collect(template, &path, out);
            continue;
        }
        let relative = path.strip_prefix(template).expect("inside template");
        if relative == Path::new(DESCRIPTION) {
            continue;
        }
        let parts: Vec<String> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_string())
            .collect();
        out.push(parts.join("/"));
    }
}
