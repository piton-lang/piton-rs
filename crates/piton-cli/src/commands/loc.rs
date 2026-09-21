//! `piton loc` — count lines of Piton source.

use std::path::PathBuf;

use piton_compile::loc;

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(paths: &[PathBuf]) -> u8 {
    let (configured, _) = project::current();
    let fallback = if configured.config_path.is_some() {
        configured.source_root.clone()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let sources = match project::collect_sources(paths, &fallback) {
        Ok(sources) => sources,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };
    if sources.is_empty() {
        println!("no .pi files found");
        return EXIT_SUCCESS;
    }

    let report = loc::count_files(&sources, Some(&configured.root));
    let width = report
        .files
        .iter()
        .map(|(path, _)| path.display().to_string().chars().count())
        .max()
        .unwrap_or(4)
        .max("file".len());

    println!(
        "{:<width$}  {:>7}  {:>7}  {:>8}  {:>7}",
        "file", "total", "code", "comments", "blank"
    );
    for (path, counts) in &report.files {
        println!(
            "{:<width$}  {:>7}  {:>7}  {:>8}  {:>7}",
            path.display().to_string(),
            counts.total,
            counts.code,
            counts.comments,
            counts.blank
        );
    }
    println!("{}", "-".repeat(width + 36));
    println!(
        "{:<width$}  {:>7}  {:>7}  {:>8}  {:>7}",
        format!("{} files", report.files.len()),
        report.summary.total,
        report.summary.code,
        report.summary.comments,
        report.summary.blank
    );
    EXIT_SUCCESS
}
