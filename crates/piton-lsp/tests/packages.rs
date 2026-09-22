//! What the editor knows about installed packages.
//!
//! A tethered package is imported by name, not by path, and it lives outside
//! the source root. Both of those are things the editor has to understand on
//! its own: completion has to offer the name, and navigation has to follow it
//! to a directory `tethers/` rather than to a sibling file.

use std::path::PathBuf;

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
    /// A project with one package installed under `tethers/`.
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("piton-lsp-pkg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let fixture = Fixture { dir };

        fixture.write(
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
        );
        fixture.write("tethers/dep-lib/index.pi", "from ./Anchors export Button\n");
        fixture.write(
            "tethers/dep-lib/Anchors.pi",
            "export anchor Button as button:\n    label: Save\n",
        );
        fixture.write("tethers/MyScope/inner/index.pi", "export anchor Inner:\n    a: 1\n");
        fixture.write(
            "spec/index.pi",
            "from dep-lib import Button\n\nexport anchor Screen:\n    primary: {Button}\n",
        );
        fixture
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
    }

    fn world(&self) -> World {
        let mut world = World::new(&self.dir);
        world.recompile(None);
        world
    }

    fn uri(&self, relative: &str) -> Url {
        path_to_url(&self.dir.join(relative)).expect("url")
    }

    /// The position just after the first occurrence of `needle`.
    fn after(&self, relative: &str, needle: &str) -> Position {
        let text = std::fs::read_to_string(self.dir.join(relative)).expect("readable");
        let offset = text
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not in {relative}"));
        offset_to_position(&text, offset + needle.len())
    }
}

fn completion_labels(items: Option<CompletionResponse>) -> Vec<String> {
    match items {
        Some(CompletionResponse::Array(items)) => {
            items.into_iter().map(|item| item.label).collect()
        }
        Some(CompletionResponse::List(list)) => {
            list.items.into_iter().map(|item| item.label).collect()
        }
        None => Vec::new(),
    }
}

#[test]
fn a_package_import_resolves_and_compiles() {
    let fixture = Fixture::new("resolves");
    let world = fixture.world();
    let compilation = world.compilation.as_ref().expect("compiled");
    let errors: Vec<&str> = compilation
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        compilation
            .graph()
            .id_for(&fixture.dir.join("tethers/dep-lib/index.pi"))
            .is_some(),
        "the package module was not loaded"
    );
}

#[test]
fn completing_a_module_path_offers_installed_packages() {
    let fixture = Fixture::new("completion");
    fixture.write("spec/Draft.pi", "from \n");
    let world = fixture.world();

    let labels = completion_labels(features::completion(
        &world,
        &fixture.uri("spec/Draft.pi"),
        fixture.after("spec/Draft.pi", "from "),
    ));

    assert!(labels.contains(&"dep-lib".to_string()), "{labels:?}");
    assert!(labels.contains(&"MyScope/inner".to_string()), "{labels:?}");
    // The bundled packages are still offered alongside them.
    assert!(labels.contains(&"@piton/belay".to_string()), "{labels:?}");
    assert!(labels.contains(&"@piton/packaging".to_string()), "{labels:?}");
    // And a file inside a package is offered by its package path, not by a
    // relative path that climbs out of the source root.
    assert!(labels.contains(&"dep-lib/Anchors".to_string()), "{labels:?}");
    assert!(
        !labels.iter().any(|label| label.contains("../tethers")),
        "{labels:?}"
    );
}

#[test]
fn going_to_a_package_import_opens_the_package() {
    let fixture = Fixture::new("goto");
    let world = fixture.world();

    let response = features::definition(
        &world,
        &fixture.uri("spec/index.pi"),
        fixture.after("spec/index.pi", "from dep"),
    )
    .expect("a definition");

    let GotoDefinitionResponse::Scalar(location) = response else {
        panic!("expected one location");
    };
    assert!(
        location.uri.to_string().ends_with("tethers/dep-lib/index.pi"),
        "{}",
        location.uri
    );
}

#[test]
fn an_installed_package_is_not_counted_as_project_source() {
    let fixture = Fixture::new("sources");
    let sources = piton_compile::module::sources(&fixture.dir);
    assert!(
        !sources
            .iter()
            .any(|path| path.starts_with(fixture.dir.join("tethers"))),
        "{sources:?}"
    );
    assert!(sources.contains(&fixture.dir.join("spec/index.pi")));
}
