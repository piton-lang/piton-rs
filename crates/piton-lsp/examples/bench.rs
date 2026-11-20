//! Measures the language server on a real project.
//!
//! Run it with `cargo run --release -p piton-lsp --example bench -- <project>
//! [file]`. It prints the duration of each phase of a first compilation, of
//! the plugin plans of a save, and of compilations after keystrokes in the
//! file (the entry without a file). With a file, it also prints the duration
//! of the requests on that file. `BENCH_ROUNDS` sets the number of
//! keystrokes.

use std::path::PathBuf;
use std::time::Instant;

use piton_lsp::features;
use piton_lsp::world::{PluginData, World};
use tower_lsp::lsp_types::{Position, Range, Url};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: bench <project> [file]");
        std::process::exit(2);
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let file = args.next().map(|file| root.join(file));

    let mut world = World::new(&root);
    world.recompile(None);
    println!("{}", piton_lsp::timings_line("first compilation", &world.timings));
    // The server plans the plugin builds after a compilation, with its parses.
    let data = PluginData::gather(&world.plan_request());
    println!("{}", piton_lsp::timings_line("planned the plugin builds", &data.timings));
    world.set_plugin_data(data);

    // Each round types one more character into a buffer, as an editor does.
    let edited = file.clone().unwrap_or_else(|| world.project.entry.clone());
    let text = std::fs::read_to_string(&edited).expect("the file can be read");
    let rounds: usize = std::env::var("BENCH_ROUNDS").ok().and_then(|n| n.parse().ok()).unwrap_or(3);
    for round in 0..rounds {
        let typed = format!("{text}\n// {}\n", "x".repeat(round + 1));
        world.open_document(edited.clone(), typed, round as i32 + 2);
        world.refresh(Some(&edited));
        println!("{}", piton_lsp::timings_line("compilation after a keystroke", &world.timings));
        if let Some(compilation) = &world.compilation {
            match compilation.reevaluated {
                Some(count) => println!("  evaluated {count} of {} anchors again", compilation.store().anchors.len()),
                None => println!("  evaluated all anchors"),
            }
        }
    }

    let Some(file) = file else {
        return;
    };

    let uri = Url::from_file_path(&file).expect("an absolute path");
    let at = Position::new(0, 1);
    let all = Range::new(Position::new(0, 0), Position::new(u32::MAX, 0));
    let time = |name: &str, run: &dyn Fn()| {
        let started = Instant::now();
        run();
        println!("{name:20} {:?}", started.elapsed());
    };
    time("hover", &|| drop(features::hover(&world, &uri, at)));
    time("definition", &|| drop(features::definition(&world, &uri, at)));
    time("semantic tokens", &|| drop(features::semantic_tokens(&world, &uri)));
    time("inlay hints", &|| drop(features::inlay_hints(&world, &uri, all)));
    time("code lenses", &|| drop(features::code_lenses(&world, &uri)));
    time("document symbols", &|| drop(features::document_symbols(&world, &uri)));
    time("completion", &|| drop(features::completion(&world, &uri, at)));
    time("workspace symbols", &|| drop(features::workspace_symbols(&world, "a")));
}
