//! Checks the JavaScript packages against the compiler they call.
//!
//! `vite-plugin-piton` and `astro-piton` are the only parts of this repository
//! that are not Rust, and neither can be run from a Rust test. What they are
//! is a caller of the `piton` binary, and a caller can drift: rename a flag
//! here and the plugin keeps spawning the old one, failing at build time in
//! someone else's project rather than in this suite.
//!
//! So the command line the plugin writes is read back out of its source and
//! checked against the compiler that has to accept it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn binary() -> PathBuf {
    // The integration binary sits next to the test binary.
    let mut path = std::env::current_exe().expect("test binary");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("piton")
}

#[test]
fn the_plugin_spawns_a_command_the_compiler_accepts() {
    let source = read("packages/vite-plugin-piton/src/compiler.ts");
    for flag in ["'compile'", "'--adapter'", "'--dependencies'"] {
        assert!(
            source.contains(flag),
            "the plugin no longer passes {flag}; the compiler contract changed"
        );
    }

    // And the compiler still takes them. A renamed flag fails here rather than
    // in a project that installed the plugin. `--adapter` is the old name of
    // `--renderer`, kept as a hidden alias, so it is run rather than looked
    // for in the help.
    let help = Command::new(binary())
        .args(["compile", "--help"])
        .output()
        .expect("piton compile --help");
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("--renderer"), "{text}");
    assert!(text.contains("--dependencies"), "{text}");

    let directory = std::env::temp_dir().join(format!("piton-plugin-flags-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("temp dir");
    std::fs::write(directory.join("a.pi"), "export anchor A:\n    x: 1\n").expect("write");
    let output = Command::new(binary())
        .args(["compile", "--adapter", "json", "--dependencies", "a.pi"])
        .current_dir(&directory)
        .output()
        .expect("piton compile --adapter");
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_plugin_offers_the_renderers_the_compiler_has() {
    // The plugin names the renderers once, in `renderers.ts`: the type it
    // accepts and the check it runs on a virtual-module path. Both have to
    // agree with the compiler, and the plugin has to use that check.
    let plugin = read("packages/vite-plugin-piton/src/index.ts");
    let renderers = read("packages/vite-plugin-piton/src/renderers.ts");

    for renderer in ["json", "yaml", "markdown"] {
        assert!(
            renderers.contains(&format!("'{renderer}'")),
            "`{renderer}` is missing from the plugin's Renderer type"
        );
        assert!(
            renderers.contains(&format!("export function {renderer}(")),
            "`{renderer}` has no renderer function"
        );
    }
    assert!(
        plugin.contains("isRenderer("),
        "virtual module paths should be checked against the renderers"
    );
    let compiler = read("packages/vite-plugin-piton/src/compiler.ts");
    assert!(compiler.contains("--renderer"), "the plugin should ask for `--renderer`");
}

#[test]
fn the_astro_integration_wraps_the_vite_plugin() {
    let astro = read("packages/astro-piton/src/index.ts");
    assert!(
        astro.contains("vite-plugin-piton"),
        "the Astro integration should add the Vite plugin rather than repeat it"
    );
    assert!(astro.contains("astro:config:setup"), "{astro}");

    let manifest = read("packages/astro-piton/package.json");
    assert!(
        manifest.contains("\"vite-plugin-piton\""),
        "the Astro package must depend on the Vite plugin it adds"
    );
    assert!(
        manifest.contains("astro-integration"),
        "an Astro integration is found by that keyword"
    );
}

#[test]
fn the_specified_packages_exist() {
    // `spec/scope/tooling/index.pi` lists these under `consumption`.
    for file in [
        "packages/vite-plugin-piton/package.json",
        "packages/vite-plugin-piton/src/index.ts",
        "packages/vite-plugin-piton/src/compiler.ts",
        "packages/vite-plugin-piton/src/renderers.ts",
        "packages/vite-plugin-piton/client.d.ts",
        "packages/vite-plugin-piton/README.md",
        "packages/astro-piton/package.json",
        "packages/astro-piton/src/index.ts",
        "packages/astro-piton/README.md",
    ] {
        assert!(
            repo_root().join(file).is_file(),
            "{file} is specified but missing"
        );
    }
}

#[test]
fn the_plugin_covers_every_feature_the_specification_lists() {
    // `spec/scope/tooling/vite/index.pi` names four, and each one is a thing
    // the plugin has to actually do rather than a thing it could claim.
    let plugin = read("packages/vite-plugin-piton/src/index.ts");
    // imports .pi files
    assert!(plugin.contains(".pi"), "{plugin}");
    // HMR
    assert!(plugin.contains("handleHotUpdate"), "{plugin}");
    // dependency tracking
    assert!(plugin.contains("addWatchFile"), "{plugin}");
    // virtual modules
    assert!(plugin.contains("virtual:piton/"), "{plugin}");
    // and the configurable default adapter the `renderers` property describes
    assert!(plugin.contains("options.adapter"), "{plugin}");
}
