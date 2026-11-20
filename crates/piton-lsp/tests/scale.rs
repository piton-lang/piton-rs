//! The server on a large generated project.
//!
//! The project has the shape that made the analysis take minutes: a package
//! of many files, exported again through `export *` indexes at two levels,
//! and an entry that exports the package again. Each lookup of an export
//! followed every chain again, and each map to the output counted lines from
//! the start of megabytes of text. The limits here are far above the normal
//! time. They catch work that grows with the square of the project.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use piton_lsp::world::World;

const GROUPS: usize = 30;
const FILES: usize = 40;

struct Project {
    dir: PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Project {
    fn generate(name: &str, renderer: &str) -> Project {
        let dir = std::env::temp_dir().join(format!("piton-scale-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |relative: &str, text: &str| {
            let path = dir.join(relative);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(path, text).expect("write");
        };
        write(
            "piton.config.pi",
            &format!("use @piton/config\n\nexport piton-config Scale:\n    root: ./spec\n    entry: ./spec/index.pi\n    renderer: {renderer}\n"),
        );
        let mut top = String::new();
        for group in 0..GROUPS {
            top.push_str(&format!("from ./g{group} export *\n"));
            let mut index = String::new();
            for file in 0..FILES {
                index.push_str(&format!("from ./R{group}x{file} export *\n"));
                write(
                    &format!("spec/standards/g{group}/R{group}x{file}.pi"),
                    &format!(
                        "from ./R{group}x{next} import R{group}x{next}\n\nexport anchor R{group}x{file}:\n    description: Rule {file} of group {group}.\n    detail:\n        A paragraph of prose that the rule explains, long enough\n        to make the output of an index file large.\n    related: @{{R{group}x{next}}}\n",
                        next = (file + 1) % FILES
                    ),
                );
            }
            write(&format!("spec/standards/g{group}/index.pi"), &index);
        }
        write("spec/standards/index.pi", &top);
        write("spec/index.pi", "from ./standards export *\n");
        Project { dir }
    }
}

fn timed(world: &mut World) -> Duration {
    world.invalidate();
    let started = Instant::now();
    world.recompile(None);
    started.elapsed()
}

fn check(renderer: &str) {
    let project = Project::generate(renderer, renderer);
    let mut world = World::new(&project.dir);
    world.recompile(None);
    let compilation = world.compilation.as_ref().expect("compiled");
    assert!(!compilation.has_errors(), "{:?}", compilation.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>());
    let again = timed(&mut world);
    // Well under a second in a debug build. Work that grows with the
    // square of the project takes many seconds here.
    assert!(again < Duration::from_secs(5), "a recompile took {again:?}: {:?}", world.timings);

    // The map to the output renders the indexes, which hold every rule.
    let file = project.dir.join("spec/standards/g0/R0x0.pi");
    let started = Instant::now();
    let mappings = world.mappings_for(&file);
    let mapped = started.elapsed();
    assert!(!mappings.is_empty(), "the rule maps to its output");
    assert!(mapped < Duration::from_secs(5), "the map took {mapped:?}");
}

#[test]
fn a_large_project_recompiles_quickly_with_json_output() {
    check("json");
}

#[test]
fn a_large_project_recompiles_quickly_with_markdown_output() {
    check("markdown");
}
