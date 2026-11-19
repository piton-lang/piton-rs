//! The files that the server knows, compared to the files that the project reaches.
//!
//! The compiler follows imports from the entry point. A file that no file
//! imports is not part of the program. An editor cannot operate like that. The
//! file is open on the screen. Usually no file imports a file that a person
//! writes now. These tests verify that the server knows each source in
//! the project, reached or not.

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
    /// A project with one reached file and one file that no file imports.
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

    /// The world with an open file, as `did_open` leaves it.
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
    // Before, the server compiled such a file as its own entry point. This
    // replaced all of the compilation of the project. While the file was
    // open, the editor did not see the rest of the specbase.
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
    // The user still edits a file that is open from a location outside the
    // project root. The editor asked for diagnostics on it.
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
    // The load of all sources from disk must not cancel the purpose of the
    // overrides. The compiler compiles the text that is on the screen.
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
    // The editor does not know about a file that a branch switch or a script
    // wrote. The editor sends `didChange` only for buffers that are open in
    // the editor. The file watcher that the server registers tells the server. The answer
    // is a new compile from disk. Thus a new source must also be in the
    // new build.
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

    // A new root with its own entry. No file below the old root is part of
    // the project now.
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

#[test]
fn a_missing_entry_still_leaves_the_workspace_to_edit() {
    let fixture = Fixture::new("missing-entry");
    std::fs::remove_file(fixture.path("spec/index.pi")).expect("remove the entry");
    fixture.write(
        "spec/Page.pi",
        "export abstract anchor Page as page:\n    title:: string\n    path:: string: /\n",
    );
    fixture.write(
        "spec/Home.pi",
        "use ./Page\n\nexport page Home:\n    title: Home\n    \n",
    );
    let world = fixture.opened("spec/Home.pi");

    // The server reports that there is no entry...
    let diagnostics = world.diagnostics_by_file();
    assert!(
        diagnostics
            .get(&fixture.path("spec/index.pi"))
            .is_some_and(|found| !found.is_empty()),
        "{diagnostics:?}"
    );
    // ...and all other files still load. The server does not report them
    // as unused. Without an entry, no file can reach them.
    let home = fixture.path("spec/Home.pi");
    assert!(loaded(&world, &home));
    assert!(loaded(&world, &fixture.path("spec/Reached.pi")));
    assert!(
        diagnostics.get(&home).is_none_or(|found| found.is_empty()),
        "{:?}",
        diagnostics.get(&home)
    );

    // Thus completion still operates in the file that the user writes.
    let text = std::fs::read_to_string(&home).expect("readable");
    let offset = text.find("    \n").expect("blank body line") + 4;
    let position = offset_to_position(&text, offset);
    let uri = path_to_url(&home).expect("url");
    let labels: Vec<String> = match features::completion(&world, &uri, position) {
        Some(CompletionResponse::Array(items)) => items.into_iter().map(|item| item.label).collect(),
        Some(CompletionResponse::List(list)) => list.items.into_iter().map(|item| item.label).collect(),
        None => Vec::new(),
    };
    // The page has `title`. Thus the list does not offer it a second
    // time. The list offers the keys that the page does not have at this time.
    assert!(labels.contains(&"path".to_string()), "{labels:?}");
    assert!(!labels.contains(&"title".to_string()), "{labels:?}");
}

/// A configuration with the removed `frameworks` key has a quick fix on the
/// `frameworks-removed` diagnostic. The fix applies the edits of the migrate
/// plugin, and moves the items to `plugins`.
#[test]
fn frameworks_removed_offers_the_migration() {
    let fixture = Fixture::new("frameworks-migration");
    let config = "use @piton/belay\nuse @piton/config\n\nfrom @piton/belay import ClaudeCodeAdapter\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n    frameworks:\n        - {Belay}\n\nbelay-config Belay:\n    codeRoot: ./src\n    adapters:\n        - {ClaudeCodeAdapter}\n";
    fixture.write("piton.config.pi", config);
    let world = fixture.world();
    let diagnostic = world
        .config_diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "frameworks-removed")
        .expect("frameworks-removed");
    let at = offset_to_position(config, diagnostic.span.start);
    let uri = path_to_url(&fixture.path("piton.config.pi")).expect("url");
    let actions = features::code_actions(&world, &uri, Range::new(at, at)).unwrap_or_default();
    let fix = actions
        .iter()
        .find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) if action.title.starts_with("Switch it over") => Some(action),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no migration in {actions:?}"));
    let edit = fix.edit.as_ref().expect("edit");
    let changes = edit.changes.as_ref().expect("changes");
    let texts: Vec<&String> = changes.values().flatten().map(|edit| &edit.new_text).collect();
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert!(texts[0].contains("plugins:") && !texts[0].contains("frameworks"), "{}", texts[0]);

    // A position away from the diagnostic offers no migration.
    let away = offset_to_position(config, 0);
    let actions = features::code_actions(&world, &uri, Range::new(away, away)).unwrap_or_default();
    assert!(actions.iter().all(|action| !matches!(action, CodeActionOrCommand::CodeAction(action) if action.title.starts_with("Switch it over"))));
}

/// A project that imports an Emul design. The workspace folder of the editor
/// is `folder`, relative to the fixture. The fixture has a configuration of
/// its own. The folder `outer` between them has none.
fn emul_world(fixture: &Fixture, folder: &str) -> (World, PathBuf) {
    fixture.write(
        "outer/project/piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "outer/project/spec/index.pi",
        "from /designs/Landing.emul import Landing\n\nexport anchor Page:\n    see: @{Landing}\n",
    );
    fixture.write(
        "outer/project/spec/designs/Landing.emul",
        "component Unused\n\nexport component Landing\n",
    );
    let path = fixture.path("outer/project/spec/index.pi");
    let text = std::fs::read_to_string(&path).expect("readable");
    let mut world = World::new(&fixture.path(folder));
    world.open_document(path.clone(), text, 1);
    world.recompile(Some(&path));
    (world, path)
}

#[test]
fn a_workspace_folder_above_the_project_finds_the_project_of_the_open_file() {
    // The editor opens a folder that holds many projects. The configuration
    // is not in the folder. The server must use the project of the open file.
    // Else the Emul plugin gives no modules and rooted imports fail.
    let fixture = Fixture::new("emul-above");
    let (world, path) = emul_world(&fixture, "outer");
    assert_eq!(world.root, fixture.path("outer/project"));
    let problems = world.diagnostics_by_file();
    assert!(
        problems.get(&path).is_none_or(Vec::is_empty),
        "{:?}",
        problems.get(&path)
    );
}

#[test]
fn a_workspace_folder_below_the_project_finds_the_configuration_above() {
    let fixture = Fixture::new("emul-below");
    let (world, path) = emul_world(&fixture, "outer/project/spec");
    assert_eq!(world.root, fixture.path("outer/project"));
    let problems = world.diagnostics_by_file();
    assert!(
        problems.get(&path).is_none_or(Vec::is_empty),
        "{:?}",
        problems.get(&path)
    );
}

#[test]
fn the_editor_checks_do_not_apply_to_the_module_of_an_emul_file() {
    // The plugin writes the Piton module of an Emul file. An unused anchor
    // or an unused import in it is not a problem that the user can fix.
    let fixture = Fixture::new("emul-module");
    let (world, _) = emul_world(&fixture, "outer/project");
    let problems = world.diagnostics_by_file();
    let emul = fixture.path("outer/project/spec/designs/Landing.emul");
    assert!(
        problems.get(&emul).is_none_or(Vec::is_empty),
        "{:?}",
        problems.get(&emul)
    );
}
