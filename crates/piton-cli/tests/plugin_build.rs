//! Plugins that build, render, and build on each other, end to end.
//!
//! These extensions make Belay a plugin. Thus this file tests each extension
//! separately, with small installed plugins. The extensions are a build plan
//! that the host writes, and the Markdown link spaces and encoders of the
//! host. They are also placeholders, a plugin that needs a different plugin,
//! and command groups under a word.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    let mut path = std::env::current_exe().expect("test binary");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("piton")
}

struct Fixture {
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("piton-plugin-build-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let fixture = Fixture { dir };
        fixture.write("piton.config.pi", CONFIG);
        fixture.write("spec/index.pi", SPEC);
        fixture.write("docs/index.ts", DOCS);
        fixture.write("helper/index.ts", HELPER);
        fixture.write("assets/run.sh", "#!/bin/sh\necho run\n");
        std::fs::set_permissions(fixture.dir.join("assets/run.sh"), std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
        fixture
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.dir.join(relative)).unwrap_or_else(|_| panic!("missing {relative}"))
    }

    fn exists(&self, relative: &str) -> bool {
        self.dir.join(relative).exists()
    }

    fn run(&self, args: &[&str]) -> (String, String, i32) {
        let output = Command::new(binary())
            .args(args)
            .current_dir(&self.dir)
            .env("NO_COLOR", "1")
            .output()
            .expect("run piton");
        (
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            output.status.code().unwrap_or(-1),
        )
    }
}

const CONFIG: &str = r#"use @piton/config
use @piton/plugin

export piton-config App:
    root: ./spec
    plugins:
        - {Docs}
        - {Helper}

piton-plugin Helper:
    name: helper
    entry: ./helper/index.ts

    capabilities:
        process: [echo]

piton-plugin Docs:
    name: docs
    entry: ./docs/index.ts
    build: true
    requires: [helper]
    placeholders: [DOCS_ROOT]

    capabilities:
        files: read
        outputs: [out/**]

    commands:
        greet:
            group: docs
            about: Greet through the helper
"#;

const SPEC: &str = "export anchor Button:\n    color: blue\n    see: @{Panel}\n\nexport anchor Panel:\n    title: Main\n";

const DOCS: &str = r#"import { definePlugin, type Context } from "@piton/plugin";

function build(ctx: Context) {
  const compiled = ctx.compile();
  const [button, panel] = compiled.anchors({ exported: true, withinSourceRoot: true });
  const path = "out/ref/index.md";
  const space = compiled.linkSpace({
    documents: [{ path, anchors: [{ id: button.id }, { id: panel.id }] }],
    files: [],
  });
  const reference = space.document(path, button.id) + "\n" + space.document(path, panel.id);
  const only = ctx.project().root.includes("only-ref");
  return {
    files: [
      { path, contents: reference, origin: "Button, Panel", label: "reference" },
      ...(only
        ? []
        : [
            { path: "out/meta.md", contents: ctx.encode("frontmatter", { name: "x: y", tags: ["a", "b"] }) },
            {
              path: "out/where.txt",
              contents: `${ctx.relativePath("out/a/b.md", path)}\n${JSON.stringify(ctx.placeholders())}\n`,
            },
          ]),
    ],
    copies: only ? [] : [{ from: "assets/run.sh", to: "out/run.sh" }],
    diagnostics: [
      { severity: "warning" as const, code: "docs-note", message: "Button has no title", location: button.location("color") },
    ],
    targets: { docs: "v1" },
  };
}

async function greet(ctx: Context) {
  const helper = ctx.require("helper") as { say: (word: string) => Promise<string> };
  ctx.stdout(await helper.say("hi"));
  try {
    ctx.spawn("echo", ["mine"]);
  } catch (error) {
    ctx.stdout(`refused: ${(error as Error).message}\n`);
  }
  try {
    ctx.require("nobody");
  } catch (error) {
    ctx.stdout(`refused: ${(error as Error).message}\n`);
  }
}

export default definePlugin({ build, commands: { greet } });
"#;

const HELPER: &str = r#"import { definePlugin, type Context } from "@piton/plugin";

export default definePlugin({
  exports: {
    say: (ctx: Context) => async (word: string) => {
      const process = ctx.spawn("echo", [word]);
      let out = "";
      for await (const { line } of process) out += `${line}\n`;
      await process.wait();
      return out;
    },
  },
});
"#;

#[test]
fn a_plugin_build_is_planned_by_the_plugin_and_written_by_the_host() {
    let fixture = Fixture::new("build");
    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");

    // The link space of the host. It has headings, and a reference with a
    // link to its target heading in the same file.
    let reference = fixture.read("out/ref/index.md");
    assert!(reference.starts_with("# Button\n"), "{reference}");
    assert!(reference.contains("[Panel](./index.md#panel)"), "{reference}");
    assert!(reference.contains("# Panel\n"), "{reference}");

    // The encoders of the host, a relative path, and the plugin placeholders.
    let meta = fixture.read("out/meta.md");
    assert!(meta.starts_with("---\n") && meta.contains("name: \"x: y\""), "{meta}");
    let where_ = fixture.read("out/where.txt");
    assert!(where_.starts_with("../ref/index.md\n"), "{where_}");
    assert!(where_.contains("\"DOCS_ROOT\":\"\u{e000}DOCS_ROOT\u{e001}\""), "{where_}");

    // A copy keeps its bytes and its execute permission.
    assert_eq!(fixture.read("out/run.sh"), "#!/bin/sh\necho run\n");
    let mode = std::fs::metadata(fixture.dir.join("out/run.sh")).unwrap().permissions().mode();
    assert_ne!(mode & 0o111, 0, "{mode:o}");

    // The diagnostic of the plan refers to the source. The manifest records
    // the written files, with the targets of the plan.
    assert!(stderr.contains("Button has no title [docs-note]"), "{stderr}");
    assert!(stderr.contains("color: blue"), "{stderr}");
    let manifest = fixture.read(".piton/manifest.json");
    assert!(manifest.contains("\"out/ref/index.md\"") && manifest.contains("\"out/run.sh\""), "{manifest}");
    assert!(manifest.contains("\"docs\": \"v1\""), "{manifest}");

    // `piton check` also makes the plan, and writes no files.
    let (_, stderr, code) = fixture.run(&["check"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("[docs-note]"), "{stderr}");
}

#[test]
fn a_later_build_removes_what_it_no_longer_writes() {
    let fixture = Fixture::new("stale");
    fixture.run(&["build"]);
    assert!(fixture.exists("out/where.txt"));
    // The same project, with a smaller plan. Its root tells the build.
    let moved = fixture.dir.with_file_name(format!("piton-plugin-build-only-ref-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&moved);
    std::fs::rename(&fixture.dir, &moved).expect("move");
    let fixture = Fixture { dir: moved };
    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!fixture.exists("out/where.txt") && !fixture.exists("out/run.sh"), "{stdout}");
    assert!(fixture.exists("out/ref/index.md"));
    assert!(stdout.contains("removed out/where.txt"), "{stdout}");
}

#[test]
fn a_dry_run_writes_nothing() {
    let fixture = Fixture::new("dry");
    let (stdout, _, code) = fixture.run(&["build", "--dry-run"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("out/ref/index.md (reference)"), "{stdout}");
    assert!(!fixture.exists("out") && !fixture.exists(".piton/manifest.json"));
}

#[test]
fn a_plan_outside_the_outputs_writes_nothing() {
    let fixture = Fixture::new("outside");
    fixture.write("docs/index.ts", &DOCS.replace("const path = \"out/ref/index.md\";", "const path = \"elsewhere/index.md\";"));
    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("plans `elsewhere/index.md`, which is not among the outputs"), "{stderr}");
    assert!(!fixture.exists("elsewhere") && !fixture.exists("out"));
}

#[test]
fn a_required_plugin_runs_with_its_own_capabilities() {
    let fixture = Fixture::new("requires");
    let (stdout, stderr, code) = fixture.run(&["docs", "greet"]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    // The helper can run echo. Thus its export runs echo. Docs cannot run it.
    assert!(stdout.starts_with("hi\n"), "{stdout}");
    assert!(stdout.contains("refused: plugin `docs` may not run `echo`"), "{stdout}");
    assert!(stdout.contains("refused: plugin `docs` requires `nobody` without listing it under requires"), "{stdout}");

    // The group lists its commands.
    let (stdout, _, _) = fixture.run(&["docs", "--help"]);
    assert!(stdout.contains("greet") && stdout.contains("Greet through the helper"), "{stdout}");
}

#[test]
fn a_missing_requirement_keeps_the_plugin_out() {
    let fixture = Fixture::new("missing");
    fixture.write("piton.config.pi", &CONFIG.replace("requires: [helper]", "requires: [ghost]"));
    let (_, stderr, _) = fixture.run(&["plugins"]);
    assert!(stderr.contains("plugin `docs` requires `ghost`, which the project doesn't have"), "{stderr}");
    let (_, stderr, code) = fixture.run(&["docs", "greet"]);
    assert_eq!(code, 2, "{stderr}");
}

#[test]
fn a_quoted_output_is_explained() {
    let fixture = Fixture::new("quoted");
    fixture.write("piton.config.pi", &CONFIG.replace("outputs: [out/**]", "outputs: [\"out/**\"]"));
    let (_, stderr, _) = fixture.run(&["plugins"]);
    assert!(stderr.contains("Piton keeps quotes as part of the text"), "{stderr}");
}
