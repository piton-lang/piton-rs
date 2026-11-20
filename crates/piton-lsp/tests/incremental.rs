//! After an edit, the server keeps the index of each module that the edit
//! did not affect, and what the analysis found in it. Each kept part must be
//! the same as a new one.

use std::path::Path;

use piton_lsp::analysis;
use piton_lsp::index::Index;
use piton_lsp::world::World;

/// Each module of an index, as text, in the order of the modules.
fn describe(index: &Index) -> Vec<String> {
    let mut out: Vec<String> = index
        .iter()
        .map(|(module, entry)| format!("{module:?} {:?} {:?}", entry.occurrences, entry.structures))
        .collect();
    out.sort();
    out
}

#[test]
fn a_kept_index_and_analysis_are_the_same_as_new_ones() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = World::new(&root);
    world.recompile(None);
    let mut files = piton_compile::module::sources(&world.project.source_root);
    files.sort();
    let mut kept = 0;
    for (round, file) in files.iter().step_by(30).enumerate() {
        let text = std::fs::read_to_string(file).expect("readable");
        for (version, edited) in [format!("{text}\n// an edit\n"), text.replacen(": ", ": Edited ", 1), text.clone()]
            .into_iter()
            .enumerate()
        {
            world.open_document(file.clone(), edited, (round * 10 + version) as i32);
            world.refresh(Some(file));
            let compilation = world.compilation.as_ref().expect("compiled");
            kept += usize::from(compilation.affected.is_some());
            let index = Index::build(compilation);
            assert_eq!(
                describe(&world.index),
                describe(&index),
                "the index after an edit of {} differs",
                file.display()
            );
            let fresh = analysis::analyze_fast(compilation, &index, &world.output, &world.plugin_plans, None);
            assert_eq!(
                format!("{:?} {:?}", world.analysis.diagnostics, world.analysis.ambiguities),
                format!("{:?} {:?}", fresh.diagnostics, fresh.ambiguities),
                "the analysis after an edit of {} differs",
                file.display()
            );
        }
        world.close_document(file);
    }
    assert!(kept > 0, "no compilation kept modules of the index before");
}
