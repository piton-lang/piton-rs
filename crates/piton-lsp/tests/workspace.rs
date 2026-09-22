//! What the server knows about, as against what the project reaches.
//!
//! The compiler follows imports from the entry point, and a file nothing
//! imports is not part of the program. An editor cannot work that way: the file
//! is open on screen, and "nothing imports this yet" is the normal state of a
//! file someone is in the middle of writing. These tests hold the server to
//! knowing about every source in the project, reached or not.

use std::path::{Path, PathBuf};

use piton_lsp::convert::{offset_to_position, path_to_url};
use piton_lsp::features;
use piton_lsp::world::World;
use tower_lsp::lsp_types::*;

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Fixture {
    /// A project with one reached file and one that nothing imports.
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("piton-lsp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("spec")).expect("temp dir");
        let fixture = Fixture { dir };

        fixture.write(
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
        );
        fixture.write("spec/index.pi", "from ./Reached export *\n");
        fixture.write(
            "spec/Reached.pi",
            "export anchor ReachedAnchor:\n    description: The entry point gets here\n",
        );
        fixture.write(
            "spec/Orphan.pi",
            "export anchor OrphanAnchor:\n    description: Nothing imports this file\n    broken: {NoSuchThing}\n",
        );
        fixture
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    fn world(&self) -> World {
        let mut world = World::new(&self.dir);
        world.recompile(None);
        world
    }

    /// The world with a file open, the way `did_open` leaves it.
    fn opened(&self, relative: &str) -> World {
        let path = self.path(relative);
        let text = std::fs::read_to_string(&path).expect("readable");
        let mut world = World::new(&self.dir);
        world.set_document(path.clone(), text);
        world.recompile(Some(&path));
        world
    }
}

fn loaded(world: &World, path: &Path) -> bool {
    world
        .compilation
        .as_ref()
        .and_then(|compilation| compilation.graph().id_for(path))
        .is_some()
}

fn symbol_names(world: &World, query: &str) -> Vec<String> {
    features::workspace_symbols(world, query)
        .unwrap_or_default()
        .into_iter()
        .map(|symbol| symbol.name)
        .collect()
}

#[test]
fn a_file_nothing_imports_is_still_part_of_the_workspace() {
    let fixture = Fixture::new("orphan-loaded");
    let world = fixture.world();

    assert!(
        loaded(&world, &fixture.path("spec/Orphan.pi")),
        "the server has to know about a file before anything imports it"
    );
    assert!(
        symbol_names(&world, "Orphan").contains(&"OrphanAnchor".to_string()),
        "and has to be able to find what it declares"
    );
}

#[test]
fn a_file_nothing_imports_still_gets_diagnostics() {
    let fixture = Fixture::new("orphan-diagnostics");
    let world = fixture.world();

    let orphan = fixture.path("spec/Orphan.pi");
    let problems = world.diagnostics_by_file();
    let reported = problems.get(&orphan).map(Vec::as_slice).unwrap_or_default();

    assert!(
        reported.iter().any(|d| d.message.contains("NoSuchThing")),
        "an unresolved reference is a problem whether or not the file is \
         imported: {reported:#?}"
    );
}

#[test]
fn opening_an_unimported_file_does_not_hide_the_project() {
    // The old fallback compiled such a file as its own entry point, which
    // replaced the project's compilation wholesale: while the file was open,
    // the rest of the specbase did not exist as far as the editor could tell.
    let fixture = Fixture::new("orphan-open");
    let world = fixture.opened("spec/Orphan.pi");

    assert!(loaded(&world, &fixture.path("spec/Orphan.pi")));
    assert!(
        loaded(&world, &fixture.path("spec/Reached.pi")),
        "opening one file is not a reason to forget the others"
    );
    assert!(
        symbol_names(&world, "Reached").contains(&"ReachedAnchor".to_string()),
        "workspace symbols still span the workspace"
    );
}

#[test]
fn an_unimported_file_can_be_navigated_like_any_other() {
    let fixture = Fixture::new("orphan-navigation");
    let world = fixture.opened("spec/Orphan.pi");

    let path = fixture.path("spec/Orphan.pi");
    let uri = path_to_url(&path).expect("url");
    let text = std::fs::read_to_string(&path).expect("readable");
    let offset = text.find("OrphanAnchor").expect("the anchor") + 2;
    let position = offset_to_position(&text, offset);

    assert!(
        features::hover(&world, &uri, position).is_some(),
        "hover needs the resolved program, and the file is now in it"
    );
    let symbols = features::document_symbols(&world, &uri);
    assert!(symbols.is_some(), "and so does the outline");
}

#[test]
fn a_buffer_from_outside_the_project_is_compiled_too() {
    // A file opened from somewhere the project root does not cover is still a
    // file being edited, and the editor has asked for diagnostics on it.
    let fixture = Fixture::new("outside");
    fixture.write(
        "notes/Scratch.pi",
        "export anchor ScratchAnchor:\n    description: Outside the source root\n",
    );

    let path = fixture.path("notes/Scratch.pi");
    let text = std::fs::read_to_string(&path).expect("readable");
    let mut world = World::new(&fixture.dir);
    world.set_document(path.clone(), text);
    world.recompile(Some(&path));

    assert!(loaded(&world, &path), "the buffer is open; it is in play");
    assert!(
        loaded(&world, &fixture.path("spec/Reached.pi")),
        "and the project it was opened next to is still there"
    );
}

#[test]
fn an_unsaved_buffer_wins_over_the_file_on_disk() {
    // Loading every source from disk must not undo the whole point of the
    // overrides: what is on screen is what gets compiled.
    let fixture = Fixture::new("unsaved");
    let path = fixture.path("spec/Orphan.pi");

    let mut world = World::new(&fixture.dir);
    world.set_document(
        path.clone(),
        "export anchor RenamedWhileEditing:\n    description: Not what is on disk\n".to_string(),
    );
    world.recompile(Some(&path));

    let names = symbol_names(&world, "Renamed");
    assert!(
        names.contains(&"RenamedWhileEditing".to_string()),
        "the buffer is the truth: {names:?}"
    );
    assert!(
        symbol_names(&world, "OrphanAnchor").is_empty(),
        "the file on disk was replaced, not added to"
    );
}

#[test]
fn a_file_written_behind_the_editors_back_is_picked_up() {
    // Nothing in the editor knows about a file a branch switch or a script
    // wrote: `didChange` fires only for buffers it has open. The server is told
    // by the file watcher it registers, and answering that means recompiling
    // from disk, so a source that appeared since the last build has to appear
    // with it.
    let fixture = Fixture::new("appeared");
    let world = fixture.world();
    assert!(!loaded(&world, &fixture.path("spec/Later.pi")));

    fixture.write(
        "spec/Later.pi",
        "export anchor WrittenLater:\n    description: Added after the server started\n",
    );

    let mut world = world;
    world.recompile(None);
    assert!(loaded(&world, &fixture.path("spec/Later.pi")));
    assert!(symbol_names(&world, "WrittenLater").contains(&"WrittenLater".to_string()));
}

#[test]
fn a_deleted_file_stops_being_part_of_the_workspace() {
    let fixture = Fixture::new("deleted");
    let mut world = fixture.world();
    assert!(loaded(&world, &fixture.path("spec/Orphan.pi")));

    std::fs::remove_file(fixture.path("spec/Orphan.pi")).expect("remove");
    world.recompile(None);

    assert!(!loaded(&world, &fixture.path("spec/Orphan.pi")));
    assert!(
        symbol_names(&world, "OrphanAnchor").is_empty(),
        "a symbol in a file that is gone is not somewhere to jump to"
    );
}

#[test]
fn rereading_the_configuration_moves_the_source_root() {
    let fixture = Fixture::new("reconfigured");
    let world = fixture.world();
    assert!(loaded(&world, &fixture.path("spec/Orphan.pi")));

    // A new root with its own entry: nothing under the old one is the project
    // any more.
    fixture.write(
        "other/index.pi",
        "export anchor Elsewhere:\n    description: A different tree\n",
    );
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./other\n    entry: ./other/index.pi\n",
    );

    let mut world = world;
    world.reload_project();
    world.recompile(None);

    assert!(loaded(&world, &fixture.path("other/index.pi")));
    assert!(
        !loaded(&world, &fixture.path("spec/Orphan.pi")),
        "the configuration says where the project is, and it moved"
    );
}
