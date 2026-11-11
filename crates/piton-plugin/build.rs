//! Embeds the files of the init plugin. The code is in `plugins/init/src`.
//! The templates are in `plugins/init/templates`.
//!
//! Each template is a directory. To add a template, add a directory. No list
//! names the templates. This script embeds each file below the plugin at its
//! path in the plugin. The plugin reads its templates from that list.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let plugin = manifest.join("../../plugins/init");
    let mut files = Vec::new();
    for directory in ["src", "templates"] {
        let root = plugin.join(directory);
        // Cargo examines the full directory. Thus a file that you edit, add
        // or remove embeds again.
        println!("cargo:rerun-if-changed={}", root.display());
        collect(&plugin, &root, &mut files);
    }
    files.sort();
    let mut out = String::from("&[\n");
    for relative in &files {
        let absolute = plugin.join(relative).canonicalize().expect("plugin file");
        writeln!(out, "    ({relative:?}, include_str!({:?})),", absolute.display().to_string()).unwrap();
    }
    out.push_str("]\n");
    let destination = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("init_files.rs");
    std::fs::write(destination, out).expect("write init_files.rs");
}

/// Returns each file below `directory` as a path relative to `plugin`, with `/`
/// as the separator.
fn collect(plugin: &Path, directory: &Path, out: &mut Vec<String>) {
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("plugin entry").path();
        if path.is_dir() {
            println!("cargo:rerun-if-changed={}", path.display());
            collect(plugin, &path, out);
            continue;
        }
        let relative = path.strip_prefix(plugin).expect("inside the plugin");
        let parts: Vec<String> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_string())
            .collect();
        out.push(parts.join("/"));
    }
}
