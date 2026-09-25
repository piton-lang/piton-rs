//! End-to-end tests over a fixture project.
//!
//! The specification exercises skills and references; this fixture adds
//! instructions, commands, agents, shape mapping, and all three adapters, so
//! every output path is covered.

use std::path::{Path, PathBuf};
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
        let dir = std::env::temp_dir().join(format!("piton-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Fixture { dir }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
    }

    fn mkdir(&self, relative: &str) {
        std::fs::create_dir_all(self.dir.join(relative)).expect("dirs");
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.dir.join(relative))
            .unwrap_or_else(|_| panic!("missing {relative}"))
    }

    fn exists(&self, relative: &str) -> bool {
        self.dir.join(relative).exists()
    }

    fn run(&self, args: &[&str]) -> (String, String, i32) {
        self.run_in(".", args)
    }

    /// Runs piton from a directory inside the fixture.
    fn run_in(&self, directory: &str, args: &[&str]) -> (String, String, i32) {
        let output = Command::new(binary())
            .args(args)
            .current_dir(self.dir.join(directory))
            .output()
            .expect("run piton");
        (
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            output.status.code().unwrap_or(-1),
        )
    }
}

/// A project with one of each construct, plus a shape tree over real source
/// directories.
fn full_project(name: &str, adapters: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fixture.mkdir("src/components/button");
    fixture.write(
        "src/components/button/Button.ts",
        "export const Button = 1;\n",
    );

    fixture.write(
        "piton.config.pi",
        &format!(
            "use @piton/config\nuse @piton/belay\n\nfrom @piton/belay import\n    ClaudeCodeAdapter,\n    CodexAdapter,\n    OpenCodeAdapter\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n\n    frameworks:\n        - {{Belay}}\n\nbelay-config Belay:\n    codeRoot: ./src\n    shapeRoot: ./spec/shape\n\n    adapters:\n{adapters}\n"
        ),
    );

    fixture.write(
        "spec/index.pi",
        "from ./Constructs export *\nfrom ./shape/components/button/Button export *\n",
    );

    fixture.write(
        "spec/Reference.pi",
        "export anchor HouseStyle:\n    description: Every component keeps its props flat.\n",
    );

    fixture.write(
        "spec/Constructs.pi",
        r#"use @piton/belay

from ./Reference import HouseStyle

export skill ReviewComponents:
    description: Review a component against the house style
    useWhen: reviewing or writing a component
    prompt:
        Read @{HouseStyle} first, then review the component.

    checklist:
        - Props are flat.
        - Names are spelled out.

export command Release:
    description: Cut a release
    prompt: Run the release checklist end to end.

export agent Reviewer:
    description: Reviews changes for style
    role: careful reviewer
    prompt:
        Look for deviations from @{HouseStyle}.
    model: fast
"#,
    );

    fixture.write(
        "spec/shape/components/button/Button.pi",
        r#"use @piton/belay

export instruction ButtonShape:
    description: How the button component is built
    prompt:
        The button takes a label and an onPress handler and nothing else.
"#,
    );

    fixture
}

#[test]
fn a_library_whose_entry_only_exports_still_builds() {
    let fixture = Fixture::new("entry-exports");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\nuse @piton/belay\n\nfrom @piton/belay import ClaudeCodeAdapter\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n\n    frameworks:\n        - {Belay}\n\nbelay-config Belay:\n    codeRoot: ./src\n\n    adapters:\n        - {ClaudeCodeAdapter}\n",
    );
    fixture.write(
        "spec/lib/UiComponent.pi",
        "export abstract anchor UiComponent as ui-component:\n    description:: string\n",
    );
    fixture.write(
        "spec/components/Button/index.pi",
        "use ../../lib/UiComponent\n\nexport ui-component Button:\n    description: A button triggers an action.\n",
    );
    fixture.write(
        "spec/components/Checkbox/index.pi",
        "use ../../lib/UiComponent\n\nexport ui-component Checkbox:\n    description: A checkbox toggles a value.\n",
    );
    fixture.write(
        "spec/index.pi",
        "from ./components/Button export Button\nfrom ./components/Checkbox export Checkbox\n",
    );

    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        fixture.exists(".claude/reference/components/Button/index.md"),
        "an exported anchor nothing links to is still compiled: {stdout}{stderr}"
    );
    assert!(
        fixture.exists(".claude/reference/components/Checkbox/index.md"),
        "{stdout}{stderr}"
    );
    assert!(
        fixture
            .read(".claude/reference/components/Button/index.md")
            .contains("A button triggers an action."),
        "the reference carries the anchor's own content"
    );
    assert!(
        !fixture.exists(".claude/reference/lib/UiComponent.md"),
        "an abstract anchor declares a shape and has no document of its own"
    );
}

#[test]
fn a_full_project_builds_every_artifact() {
    let fixture = full_project("full", "        - {ClaudeCodeAdapter}");
    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "stdout: {stdout}\nstderr: {stderr}");

    // Skill.
    let skill = fixture.read(".claude/skills/review-components/SKILL.md");
    assert!(
        skill.starts_with("---\nname: review-components\n"),
        "{skill}"
    );
    assert!(
        // The discovery description is the description followed by `Use when`
        // and useWhen, exactly as written.
        skill.contains("description: Review a component against the house style Use when reviewing or writing a component"),
        "{skill}"
    );
    assert!(
        skill.contains("# Checklist"),
        "additional properties serialize after the prompt:\n{skill}"
    );
    assert!(skill.contains("- Props are flat."), "{skill}");

    // A reference in a prompt becomes a link to the anchor's heading in the
    // reference file for its module.
    assert!(
        skill.contains("[HouseStyle](../../reference/Reference.md#house-style)"),
        "{skill}"
    );
    assert!(fixture.exists(".claude/reference/Reference.md"));

    // Command: an x-prefixed skill with automatic invocation disabled.
    let command = fixture.read(".claude/skills/x-release/SKILL.md");
    assert!(command.contains("name: x-release"), "{command}");
    assert!(
        command.contains("disable-model-invocation: true"),
        "{command}"
    );
    assert!(
        !fixture.exists(".claude/commands/x-release.md"),
        "one representation only"
    );

    // Agent: role introduction, then prompt, with explicit native options only.
    let agent = fixture.read(".claude/agents/reviewer.md");
    assert!(agent.contains("name: reviewer"), "{agent}");
    assert!(agent.contains("model: fast"), "{agent}");
    assert!(agent.contains("You are a careful reviewer"), "{agent}");
    assert!(
        !agent.contains("tools:"),
        "absent options stay absent:\n{agent}"
    );

    // Instruction: placed at the matching code scope, not at the shape path.
    let instruction = fixture.read("src/components/button/CLAUDE.md");
    assert!(instruction.contains("# Button Shape"), "{instruction}");
    assert!(
        instruction.contains("label and an onPress handler"),
        "{instruction}"
    );
    // And preserved in the compiled shape tree.
    assert!(fixture.exists(".claude/reference/shape/components/button/Button.md"));
}

#[test]
fn an_instruction_falls_back_to_the_nearest_existing_scope() {
    let fixture = full_project("fallback", "        - {ClaudeCodeAdapter}");
    fixture.write(
        "spec/shape/components/missing/Ghost.pi",
        "use @piton/belay\n\nexport instruction GhostShape:\n    description: For a component that does not exist yet\n    prompt: Keep it simple.\n",
    );
    fixture.write(
        "spec/index.pi",
        "from ./Constructs export *\nfrom ./shape/components/button/Button export *\nfrom ./shape/components/missing/Ghost export *\n",
    );
    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");

    // `src/components/missing` does not exist, so placement walks up.
    assert!(!fixture.exists("src/components/missing/CLAUDE.md"));
    let combined = fixture.read("src/components/CLAUDE.md");
    assert!(combined.contains("# Ghost Shape"), "{combined}");
    // The original location survives in the shape tree.
    assert!(fixture.exists(".claude/reference/shape/components/missing/Ghost.md"));
}

#[test]
fn every_adapter_emits_its_own_native_form() {
    let fixture = full_project(
        "adapters",
        "        - {ClaudeCodeAdapter}\n        - {CodexAdapter}\n        - {OpenCodeAdapter}\n\n    crossDiscovery: allow",
    );
    // OpenCode wants a provider-qualified model, and only loads a nested
    // AGENTS.md that its configuration lists.
    let constructs = fixture.read("spec/Constructs.pi").replace("model: fast", "model: anthropic/fast");
    fixture.write("spec/Constructs.pi", &constructs);
    fixture.write(
        "opencode.json",
        "{\n  \"instructions\": [\"src/components/button/AGENTS.md\"]\n}\n",
    );
    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");

    // Codex: a command is a skill plus an explicit-invocation policy file.
    assert!(fixture.exists(".agents/skills/x-release/SKILL.md"));
    let policy = fixture.read(".agents/skills/x-release/agents/openai.yaml");
    assert_eq!(policy, "policy:\n  allow_implicit_invocation: false\n");

    // Codex agents are TOML with the three identity and instruction fields.
    let toml = fixture.read(".codex/agents/reviewer.toml");
    assert!(toml.contains("name = \"reviewer\""), "{toml}");
    assert!(toml.contains("description = "), "{toml}");
    assert!(toml.contains("developer_instructions = "), "{toml}");
    assert!(toml.contains("You are a careful reviewer"), "{toml}");
    // `model` is not a documented option for the standalone agent format here,
    // so it must not be invented.
    assert!(
        toml.contains("model = \"anthropic/fast\""),
        "model is supported: {toml}"
    );

    // OpenCode: commands are native and agent mode is explicit.
    let command = fixture.read(".opencode/commands/x-release.md");
    assert!(command.contains("description: Cut a release"), "{command}");
    assert!(
        !command.contains("name:"),
        "identity comes from the filename:\n{command}"
    );
    let agent = fixture.read(".opencode/agents/reviewer.md");
    assert!(agent.contains("mode: subagent"), "{agent}");

    // Codex and OpenCode share AGENTS.md; identical guidance is written once.
    assert!(fixture.exists("src/components/button/AGENTS.md"));

    // Cross-target discovery is reported, not silently accepted.
    assert!(
        stderr.contains("cross-target-discovery"),
        "expected a discovery warning:\n{stderr}"
    );
}

#[test]
fn check_reports_problems_and_exits_non_zero() {
    let fixture = Fixture::new("check");
    fixture.write(
        "broken.pi",
        "export anchor Thing:\n    value: {NotDefined}\n",
    );
    let (_, stderr, code) = fixture.run(&["check", "broken.pi"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("`NotDefined` is not in scope"), "{stderr}");
    assert!(stderr.contains("unresolved-symbol"), "{stderr}");
}

#[test]
fn check_passes_on_a_clean_file() {
    let fixture = Fixture::new("clean");
    fixture.write("ok.pi", "export anchor Thing:\n    value: 42\n");
    let (_, stderr, code) = fixture.run(&["check", "ok.pi"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("no problems found"), "{stderr}");
}

#[test]
fn compile_renders_each_adapter() {
    let fixture = Fixture::new("compile");
    fixture.write(
        "data.pi",
        "export anchor Thing:\n    name: Widget\n    count: 3\n    ready: true\n    missing: null\n    tags: [a, b]\n",
    );

    let (json, stderr, code) = fixture.run(&["compile", "data.pi", "--renderer", "json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(json.contains("\"count\": 3"), "{json}");
    assert!(json.contains("\"ready\": true"), "{json}");
    assert!(json.contains("\"missing\": null"), "{json}");
    assert!(json.contains("\"tags\": [\n"), "{json}");

    let (yaml, _, code) = fixture.run(&["compile", "data.pi", "--renderer", "yaml"]);
    assert_eq!(code, 0);
    assert!(yaml.contains("name: Widget"), "{yaml}");
    assert!(yaml.contains("count: 3"), "{yaml}");

    let (markdown, _, code) = fixture.run(&["compile", "data.pi", "--renderer", "markdown"]);
    assert_eq!(code, 0);
    assert!(markdown.contains("# Thing"), "{markdown}");
    assert!(markdown.contains("## Name"), "{markdown}");
    assert!(markdown.contains("## Count"), "{markdown}");
}

#[test]
fn compile_includes_what_the_file_exports_without_declaring() {
    let fixture = Fixture::new("compile-reexport");
    fixture.write("One.pi", "export anchor One:\n    value: 1\n");
    fixture.write("Two.pi", "export anchor Two:\n    value: 2\n");
    fixture.write(
        "index.pi",
        "from ./One export *\nfrom ./Two export Two Renamed\n",
    );

    let (json, stderr, code) = fixture.run(&["compile", "index.pi", "--renderer", "json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        json.contains("\"One\""),
        "an index that only forwards still compiles to what it forwards: {json}"
    );
    assert!(
        json.contains("\"Renamed\""),
        "an export is compiled under the name it leaves by: {json}"
    );
}

#[test]
fn compile_writes_next_to_the_input_when_asked() {
    let fixture = Fixture::new("compile-write");
    fixture.write("data.pi", "export anchor Thing:\n    name: Widget\n");
    let (_, stderr, code) = fixture.run(&["compile", "data.pi", "--renderer", "yaml", "--write"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(fixture.read("data.yaml").contains("name: Widget"));
}

#[test]
fn a_glob_without_write_is_refused() {
    let fixture = Fixture::new("glob");
    fixture.write("a.pi", "export anchor A:\n    x: 1\n");
    fixture.write("b.pi", "export anchor B:\n    x: 2\n");
    let (_, stderr, code) = fixture.run(&["compile", "*.pi", "--renderer", "json"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("needs --write"), "{stderr}");

    let (_, stderr, code) = fixture.run(&["compile", "*.pi", "--renderer", "json", "--write"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(fixture.exists("a.json") && fixture.exists("b.json"));
}

#[test]
fn format_normalizes_and_check_reports() {
    let fixture = Fixture::new("format");
    fixture.write(
        "messy.pi",
        "from ./x import C, A, B\nanchor A:\n\tvalue: 1 //tight\n",
    );
    fixture.write(
        "x.pi",
        "export anchor A:\n    v: 1\nexport anchor B:\n    v: 2\nexport anchor C:\n    v: 3\n",
    );

    let (_, stderr, code) = fixture.run(&["format", "messy.pi", "--check"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("unformatted"), "{stderr}");

    let (_, _, code) = fixture.run(&["format", "messy.pi"]);
    assert_eq!(code, 0);
    let formatted = fixture.read("messy.pi");
    assert_eq!(
        formatted,
        "from ./x import\n    A,\n    B,\n    C\nanchor A:\n    value: 1 //tight\n"
    );

    let (_, _, code) = fixture.run(&["format", "messy.pi", "--check"]);
    assert_eq!(code, 0, "formatting is idempotent");
}

#[test]
fn loc_counts_by_category() {
    let fixture = Fixture::new("loc");
    fixture.write(
        "counted.pi",
        "// a comment\n\nexport anchor A:\n    value: 1 // trailing\n",
    );
    let (stdout, _, code) = fixture.run(&["loc", "counted.pi"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("total"), "{stdout}");
    let summary = stdout.lines().last().unwrap_or_default();
    // total 4, code 2, comments 1, blank 1
    assert!(summary.contains(" 4 "), "{stdout}");
}

#[test]
fn reach_separates_reachable_from_unreachable() {
    let fixture = full_project("reach", "        - {ClaudeCodeAdapter}");
    fixture.write(
        "spec/Orphan.pi",
        "export anchor NobodyImportsThis:\n    value: 1\n",
    );
    let (stdout, stderr, code) = fixture.run(&["reach"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("reachable"), "{stdout}");
    assert!(stdout.contains("ReviewComponents"), "{stdout}");
    // Nothing imports the orphan, so it is not even loaded.
    assert!(!stdout.contains("NobodyImportsThis"), "{stdout}");
}

#[test]
fn reach_does_not_call_an_index_dead_for_declaring_nothing() {
    // A module that only re-exports declares no anchor of its own, so a
    // reachability walk that counts only declarations finds nothing in it and
    // calls it unreachable -- while the whole import chain runs through it.
    let fixture = full_project("reach-index", "        - {ClaudeCodeAdapter}");
    fixture.write(
        "spec/index.pi",
        "from ./barrel export *\nfrom ./shape/components/button/Button export *\n",
    );
    fixture.write("spec/barrel/index.pi", "from ./Constructs export *\n");
    fixture.write(
        "spec/barrel/Constructs.pi",
        "use @piton/belay\n\nexport command Release:\n    description: Cut a release\n    prompt: Run the release checklist end to end.\n",
    );

    let (stdout, stderr, code) = fixture.run(&["reach"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("Release"), "{stdout}");

    let unreachable = stdout.split("\nunreachable").nth(1).unwrap_or_default();
    assert!(
        !unreachable.contains("barrel/index.pi"),
        "an index that carries a reachable anchor is not dead: {stdout}"
    );
}

#[test]
fn reach_counts_every_line_it_prints() {
    let fixture = full_project("reach-counts", "        - {ClaudeCodeAdapter}");
    // Loaded, because a file the entry does not reach imports it, and nothing
    // reachable uses what it declares: a dead anchor in a file that is
    // load-bearing for nothing.
    fixture.write(
        "spec/Dead.pi",
        "export anchor NothingUsesThis:\n    value: 1\n",
    );
    fixture.write("spec/Keywords.pi", "from ./Dead import NothingUsesThis\n");
    fixture.write(
        "spec/index.pi",
        "use ./Keywords\n\nfrom ./Constructs export *\nfrom ./shape/components/button/Button export *\n",
    );

    let (stdout, stderr, code) = fixture.run(&["reach"]);
    assert_eq!(code, 0, "{stderr}");

    let unreachable = stdout.split("\nunreachable").nth(1).unwrap_or_default();
    assert!(unreachable.contains("NothingUsesThis"), "{stdout}");

    // The summary has to agree with the listing: an anchor count that leaves
    // out the module lines printed under the same heading is what made the
    // report hard to trust.
    let anchors = unreachable.matches("NothingUsesThis").count();
    assert_eq!(anchors, 1, "{stdout}");
    assert!(
        stdout.contains("1 unreachable"),
        "one dead anchor, counted once: {stdout}"
    );
    assert!(
        stdout.contains("2 carrying nothing reachable"),
        "and the file it is in and the file importing it, counted separately: {stdout}"
    );
}

#[test]
fn reach_follows_imports_and_reports_paths_by_default() {
    let fixture = full_project("reach-imports", "        - {ClaudeCodeAdapter}");
    fixture.write(
        "spec/Imported.pi",
        "export anchor OnlyImported:\n    value: 1\n",
    );
    fixture.write(
        "spec/index.pi",
        "from ./Constructs export *\nfrom ./shape/components/button/Button export *\nfrom ./Imported import OnlyImported\n",
    );

    let (stdout, stderr, code) = fixture.run(&["reach"]);
    assert_eq!(code, 0, "{stderr}");
    // Imported by the entry and used by nothing: reached through the import.
    assert!(
        stdout.contains("OnlyImported  depth 1  via imports  [index.pi -> OnlyImported]"),
        "{stdout}"
    );
    // Paths and the unreachable half are on without asking.
    assert!(stdout.contains(" -> "), "{stdout}");
    assert!(stdout.contains("\nunreachable"), "{stdout}");

    let (stdout, _, code) = fixture.run(&["reach", "--no-paths", "--no-unreachable"]);
    assert_eq!(code, 0);
    assert!(!stdout.contains(" -> "), "{stdout}");
    assert!(!stdout.contains("\nunreachable\n"), "{stdout}");
    assert!(stdout.contains("OnlyImported  depth 1  via imports\n"), "{stdout}");
}

#[test]
fn reach_accepts_a_glob() {
    let fixture = full_project("reach-glob", "        - {ClaudeCodeAdapter}");
    let (stdout, stderr, code) = fixture.run(&["reach", "spec/Ref*.pi"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("HouseStyle  depth 0  root"), "{stdout}");
}

#[test]
fn build_without_a_framework_writes_the_renderer_output() {
    let fixture = Fixture::new("build-plain");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n",
    );
    fixture.write(
        "spec/index.pi",
        "from ./components/Button import Button\n\nexport anchor Screen:\n    primary: {Button}\n",
    );
    fixture.write(
        "spec/components/Button.pi",
        "export anchor Button:\n    label: Save\n",
    );
    fixture.write(
        "spec/Unused.pi",
        "export anchor Unused:\n    label: nobody\n",
    );

    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(fixture.exists("dist/index.json"), "{stdout}");
    assert!(
        fixture.exists("dist/components/Button.json"),
        "a file the entry's exports reference is written: {stdout}"
    );
    assert!(fixture.read("dist/components/Button.json").contains("Save"));
    assert!(!fixture.exists("dist/Unused.json"), "nothing references it");

    let manifest = fixture.read(".piton/manifest.json");
    assert!(manifest.contains("\"dist/index.json\""), "{manifest}");
    assert!(manifest.contains("\"dist/components/Button.json\""), "{manifest}");
}

#[test]
fn build_uses_the_configured_output_and_renderer() {
    let fixture = Fixture::new("build-yaml");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    output: ./out\n    renderer: yaml\n",
    );
    fixture.write("spec/index.pi", "export anchor Screen:\n    label: Hi\n");
    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(fixture.read("out/index.yaml").contains("label: Hi"));
    assert!(!fixture.exists("dist"));
}

#[test]
fn a_framework_replaces_the_renderer_output() {
    let fixture = full_project("build-framework", "        - {ClaudeCodeAdapter}");
    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(fixture.exists(".claude/skills/review-components/SKILL.md"));
    assert!(!fixture.exists("dist"), "only the adapter's output is written");
    let manifest = fixture.read(".piton/manifest.json");
    assert!(!manifest.contains("dist/"), "{manifest}");
}

#[test]
fn naming_the_renderer_output_writes_it_alongside_a_framework() {
    for (name, setting) in [("build-both-output", "output: ./dist"), ("build-both-renderer", "renderer: yaml")] {
        let fixture = full_project(name, "        - {ClaudeCodeAdapter}");
        let config = fixture
            .read("piton.config.pi")
            .replace("    entry: ./spec/index.pi\n", &format!("    entry: ./spec/index.pi\n    {setting}\n"));
        fixture.write("piton.config.pi", &config);

        let (_, stderr, code) = fixture.run(&["build"]);
        assert_eq!(code, 0, "{stderr}");
        let extension = if setting.starts_with("renderer") { "yaml" } else { "json" };
        assert!(fixture.exists(&format!("dist/index.{extension}")), "{setting}");
        assert!(fixture.exists(".claude/skills/review-components/SKILL.md"));
        let manifest = fixture.read(".piton/manifest.json");
        assert!(manifest.contains(&format!("\"dist/index.{extension}\"")), "{manifest}");

        // Taking the setting out again removes what it wrote.
        let without = config.replace(&format!("    {setting}\n"), "");
        fixture.write("piton.config.pi", &without);
        let (stdout, stderr, code) = fixture.run(&["build"]);
        assert_eq!(code, 0, "{stderr}");
        assert!(!fixture.exists(&format!("dist/index.{extension}")), "{stdout}");
    }
}

#[test]
fn an_unknown_config_key_is_a_warning() {
    let fixture = Fixture::new("config-typo");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    rendrer: yaml\n",
    );
    fixture.write("spec/index.pi", "export anchor A:\n    x: 1\n");
    let (_, stderr, code) = fixture.run(&["check"]);
    assert_eq!(code, 0, "a warning, not an error: {stderr}");
    assert!(stderr.contains("unknown-config-key"), "{stderr}");
    assert!(stderr.contains("rendrer"), "{stderr}");
}

#[test]
fn check_without_a_config_checks_everything_under_the_working_directory() {
    let fixture = Fixture::new("check-all");
    fixture.write("ok.pi", "export anchor Fine:\n    value: 1\n");
    fixture.write(
        "nested/broken.pi",
        "export anchor Thing:\n    value: {NotDefined}\n",
    );
    let (_, stderr, code) = fixture.run(&["check"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("`NotDefined` is not in scope"), "{stderr}");
}

#[test]
fn format_reads_stdin_when_given_a_dash() {
    let mut child = Command::new(binary())
        .args(["format", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(b"from ./x.pi import B, A\nanchor A:\n  value: 1 //tight\n")
            .expect("write");
    }
    let output = child.wait_with_output().expect("wait");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "from ./x import A, B\nanchor A:\n    value: 1 //tight\n"
    );

    let mut child = Command::new(binary())
        .args(["format", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(b"from import\n").expect("write");
    }
    let output = child.wait_with_output().expect("wait");
    assert!(!output.status.success(), "a broken source fails");
    assert!(output.stdout.is_empty(), "and writes nothing to stdout");
}

#[test]
fn agent_prints_its_fluency_without_a_project() {
    let fixture = Fixture::new("fluency-bare");
    let (stdout, stderr, code) = fixture.run(&["agent", "--print-fluency"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("# Piton Fluency"), "{stdout}");

    // A project's own FLUENCY_PROMPT.md is the one it gets.
    fixture.write("FLUENCY_PROMPT.md", "# Project Fluency\n\nlocal words\n");
    let (stdout, stderr, code) = fixture.run(&["agent", "--print-fluency"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("local words"), "{stdout}");
    assert!(!stdout.contains("# Piton Fluency"), "{stdout}");
}

#[test]
fn reach_marks_a_root_as_a_root() {
    let fixture = full_project("reach-root", "        - {ClaudeCodeAdapter}");
    let (stdout, stderr, code) = fixture.run(&["reach", "ReviewComponents"]);
    assert_eq!(code, 0, "{stderr}");
    // A root was not reached from anywhere, so naming an edge it arrived on
    // is naming an edge that does not exist.
    assert!(
        stdout.contains("ReviewComponents  depth 0  root"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("depth 0  via"),
        "nothing arrives at depth 0: {stdout}"
    );
}

#[test]
fn build_cleans_up_only_what_it_generated() {
    let fixture = full_project("cleanup", "        - {ClaudeCodeAdapter}");
    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(fixture.exists(".claude/skills/x-release/SKILL.md"));

    // A file the user wrote inside an output directory must survive.
    fixture.write(
        ".claude/skills/mine/SKILL.md",
        "---\nname: mine\n---\n\nhand written\n",
    );

    // Remove the command from the source and rebuild.
    let source = fixture.read("spec/Constructs.pi");
    let trimmed = source.replace(
        "export command Release:\n    description: Cut a release\n    prompt: Run the release checklist end to end.\n\n",
        "",
    );
    assert_ne!(trimmed, source, "the command block should have been found");
    fixture.write("spec/Constructs.pi", &trimmed);

    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("removed"), "{stdout}");
    assert!(!fixture.exists(".claude/skills/x-release/SKILL.md"));
    assert!(
        fixture.exists(".claude/skills/mine/SKILL.md"),
        "cleanup must never touch a file Belay did not write"
    );
}

#[test]
fn a_dry_run_writes_nothing() {
    let fixture = full_project("dry", "        - {ClaudeCodeAdapter}");
    let (stdout, stderr, code) = fixture.run(&["build", "--dry-run"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains(".claude/skills/review-components/SKILL.md"),
        "{stdout}"
    );
    assert!(!fixture.exists(".claude/skills/review-components/SKILL.md"));
}

#[test]
fn an_unknown_adapter_is_rejected() {
    let fixture = Fixture::new("badadapter");
    fixture.write("a.pi", "export anchor A:\n    x: 1\n");
    let (_, stderr, code) = fixture.run(&["compile", "a.pi", "--renderer", "toml"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("unknown renderer `toml`"), "{stderr}");
}

#[test]
fn a_relative_file_argument_resolves() {
    // The module graph is keyed by absolute paths, so a relative argument has
    // to be resolved before it will match anything.
    let fixture = Fixture::new("relative-target");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write(
        "spec/index.pi",
        "from ./Design import Design\n\nexport anchor Root:\n    uses: {Design}\n",
    );
    fixture.write(
        "spec/Design.pi",
        "export anchor Design:\n    palette: blue\n",
    );

    for target in ["spec/Design.pi", "./spec/Design.pi"] {
        let (stdout, stderr, code) = fixture.run(&["reach", target]);
        assert_eq!(code, 0, "`{target}` should be found:\n{stderr}");
        assert!(stdout.contains("Design"), "`{target}`:\n{stdout}");
    }
}

#[test]
fn help_lists_every_documented_command() {
    let output = Command::new(binary()).arg("--help").output().expect("help");
    let text = String::from_utf8_lossy(&output.stdout);
    for command in [
        "agent", "build", "check", "compile", "format", "init", "loc", "lsp", "reach", "slice",
    ] {
        assert!(
            text.contains(command),
            "`{command}` missing from help:\n{text}"
        );
    }
    let _ = Path::new(".");
}

/// Two files where a button reads and references a document, with an
/// unrelated sibling property.
fn slice_project(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fixture.write(
        "spec/Document.pi",
        "export anchor Document:\n    name: Untitled\n    save: Writes it to disk.\n",
    );
    fixture.write(
        "spec/ui.pi",
        "from ./Document import Document\n\n\
         export anchor SaveButton:\n    \
         color: blue\n    \
         click: Saves ${Document.name} with @{Document.save}.\n",
    );
    fixture
}

#[test]
fn slice_prints_what_a_target_depends_on() {
    let fixture = slice_project("slice");

    let (stdout, stderr, code) = fixture.run(&["slice", "spec/ui.pi#SaveButton.click"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with("# SaveButton.click\n"), "{stdout}");
    assert!(
        stdout.contains("### SaveButton.click\n\nReads `Document.name`.\n\nSaves Untitled with Document.save.\n"),
        "{stdout}"
    );
    assert!(stdout.contains("### Document.save\n"), "{stdout}");
    assert!(!stdout.contains("SaveButton.color"), "{stdout}");

    // A property with no dependencies is a slice of one.
    let (stdout, _, code) = fixture.run(&["slice", "spec/ui.pi#SaveButton.color"]);
    assert_eq!(code, 0);
    assert!(!stdout.contains("Document"), "{stdout}");

    // A bare name is looked up across the project, and gives the same bytes.
    let (bare, stderr, code) = fixture.run(&["slice", "SaveButton.color"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(bare, stdout);
}

#[test]
fn slice_reports_targets_it_cannot_resolve() {
    let fixture = slice_project("slice-errors");
    fixture.write("spec/other.pi", "export anchor SaveButton:\n    color: red\n");

    let cases: [(&[&str], &str); 5] = [
        (&["slice", "spec/missing.pi#SaveButton"], "does not exist"),
        (&["slice", "spec/ui.pi#SaveButon"], "[unresolved-symbol]"),
        (&["slice", "spec/ui.pi#SaveButton.colour"], "did you mean `color`?"),
        (&["slice", "SaveButton"], "[ambiguous-target]"),
        (&["slice", "spec/ui.pi"], "write `spec/ui.pi#Name`"),
    ];
    for (args, expected) in cases {
        let (stdout, stderr, code) = fixture.run(args);
        assert_eq!(code, 1, "{args:?} should fail");
        assert!(stdout.is_empty(), "{args:?}: {stdout}");
        assert!(stderr.contains(expected), "{args:?}:\n{stderr}");
    }
}

/// A Belay project whose build compiles a skill and the reference it cites,
/// and an anchor that reads one the build never writes out, plus a file
/// nothing imports.
fn slice_adapter_project(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fixture.write(
        "piton.config.pi",
        "use @piton/config\nuse @piton/belay\n\nfrom @piton/belay import ClaudeCodeAdapter\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n\n    frameworks:\n        - {Belay}\n\nbelay-config Belay:\n    codeRoot: ./src\n\n    adapters:\n        - {ClaudeCodeAdapter}\n",
    );
    fixture.write(
        "spec/Reference.pi",
        "export anchor HouseStyle:\n    description: Every component keeps its props flat.\n    naming: Spell names out.\n",
    );
    fixture.write(
        "spec/Orphan.pi",
        "export anchor Orphan:\n    note: Nothing the build emits cites this.\n",
    );
    fixture.write(
        "spec/Constructs.pi",
        "use @piton/belay\n\nfrom ./Reference import HouseStyle\nfrom ./Orphan import Orphan\n\n\
         export skill ReviewComponents:\n    \
         description: Review a component\n    \
         useWhen: reviewing a component\n    \
         prompt: Read @{HouseStyle.naming} first, then review.\n\n\
         export anchor Uses:\n    \
         note: See ${Orphan.note}\n",
    );
    fixture.write("spec/Loose.pi", "export anchor Loose:\n    note: Unimported.\n");
    fixture.write("spec/index.pi", "from ./Constructs export ReviewComponents, Uses\n");
    fixture
}

#[test]
fn slice_with_an_adapter_cites_what_the_build_writes() {
    let fixture = slice_adapter_project("slice-adapter");

    let (stdout, stderr, code) = fixture.run(&[
        "slice",
        "spec/Constructs.pi#ReviewComponents.prompt",
        "--adapter",
        "claude-code",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("Anchor in `.claude/skills/review-components/SKILL.md`"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Read [HouseStyle.naming](.claude/reference/Reference.md#naming) first"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Anchor in `.claude/reference/Reference.md#house-style`"),
        "{stdout}"
    );
    // An abstract base has no document of its own; what it declares is
    // compiled into the anchors that extend it, and the source is not cited.
    assert!(
        stdout.contains("Abstract anchor, compiled into the anchors that extend it."),
        "{stdout}"
    );
    assert!(!stdout.contains("spec/"), "{stdout}");
    assert!(!stdout.contains("@piton/"), "{stdout}");

    // Nor is it for an anchor that is only read: its value is compiled into
    // the reader's document.
    let (uses, stderr, code) =
        fixture.run(&["slice", "spec/Constructs.pi#Uses", "--adapter", "claude-code"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(uses.contains("Anchor in `.claude/reference/Constructs.md#uses`"), "{uses}");
    assert!(uses.contains("## Orphan\n\nAnchor with no compiled document of its own.\n"), "{uses}");
    assert!(!uses.contains(".pi"), "{uses}");

    // Every location cited is one the build really writes.
    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(fixture.exists(".claude/skills/review-components/SKILL.md"));
    assert!(fixture
        .read(".claude/reference/Reference.md")
        .contains("## Naming"));

    // A bare name gives the same bytes.
    let (bare, stderr, code) = fixture.run(&[
        "slice",
        "ReviewComponents.prompt",
        "--adapter",
        "claude-code",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(bare, stdout);
}

#[test]
fn slice_with_an_adapter_finds_an_anchor_embedded_upstream() {
    // `Leaf` has no document of its own: the build inlines it into
    // `Group.items`, which is itself inlined into `Index.group`. Nothing the
    // slice of `Leaf` depends on says so, since what embeds it is upstream.
    let fixture = slice_adapter_project("slice-adapter-embedded");
    fixture.write(
        "spec/Group.pi",
        "export anchor Leaf:\n    note: Only ever embedded.\n\n\
         export anchor Group:\n    \
         intro: A group of leaves.\n    \
         items:\n        - {Leaf}\n",
    );
    fixture.write(
        "spec/index.pi",
        "from ./Constructs export ReviewComponents\nfrom ./Group import Group\n\n\
         export anchor Index:\n    group: {Group}\n",
    );

    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    let index = fixture.read(".claude/reference/index.md");
    assert!(index.contains("### Items\n"), "{index}");
    assert!(index.contains("Only ever embedded."), "{index}");

    let (stdout, stderr, code) = fixture.run(&[
        "slice",
        "spec/Group.pi#Leaf",
        "--adapter",
        "claude-code",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("Anchor embedded in `.claude/reference/index.md#items`"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "The entry reaches it through [`Index.group`](.claude/reference/index.md#group) and [`Group.items`](.claude/reference/index.md#items), which are included too."
        ),
        "{stdout}"
    );

    // Citing the source, the chain is still traced from the project's entry,
    // not from the file the target is in.
    let (source, stderr, code) = fixture.run(&["slice", "spec/Group.pi#Leaf"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        source.contains("The entry reaches it through `Index.group` and `Group.items`"),
        "{source}"
    );
    assert!(source.contains("## Index\n\nAnchor in `spec/index.pi`."), "{source}");
    assert!(!source.contains("A group of leaves."), "{source}");
}

#[test]
fn slice_writes_every_path_relative_to_where_it_runs() {
    // Run from spec/, below the configuration, which slice finds by looking
    // upward.
    let fixture = slice_adapter_project("slice-here");

    let (stdout, stderr, code) = fixture.run_in(
        "spec",
        &["slice", "Constructs.pi#ReviewComponents.prompt", "--adapter", "claude-code"],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("Anchor in `../.claude/skills/review-components/SKILL.md`"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Read [HouseStyle.naming](../.claude/reference/Reference.md#naming) first"),
        "{stdout}"
    );

    let (stdout, stderr, code) = fixture.run_in("spec", &["slice", "Constructs.pi#Uses"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("Anchor in `Constructs.pi`"), "{stdout}");
    assert!(stdout.contains("## Orphan\n\nAnchor in `Orphan.pi`"), "{stdout}");

    fixture.mkdir("src/deep");
    let (stdout, stderr, code) = fixture.run_in("src/deep", &["slice", "Uses"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("Anchor in `../../spec/Constructs.pi`"), "{stdout}");
}

#[test]
fn slice_with_an_adapter_needs_a_target_the_build_writes() {
    let fixture = slice_adapter_project("slice-adapter-errors");

    let cases: [(&[&str], &[&str]); 4] = [
        (
            &["slice", "spec/Orphan.pi#Orphan.note", "--adapter", "claude-code"],
            &["`Orphan` is never compiled"],
        ),
        (
            &["slice", "spec/Loose.pi#Loose", "--adapter", "claude-code"],
            &["is not part of what the build compiles"],
        ),
        (
            &["slice", "Loose", "--adapter", "claude-code"],
            &["[unresolved-symbol]", "only what the build compiles is searched"],
        ),
        (
            &["slice", "Loose", "--adapter", "codex"],
            &["`codex` is not a target this project builds; it builds `claude-code`"],
        ),
    ];
    for (args, expected) in cases {
        let (stdout, stderr, code) = fixture.run(args);
        assert_eq!(code, 1, "{args:?} should fail");
        assert!(stdout.is_empty(), "{args:?}: {stdout}");
        for text in expected {
            assert!(stderr.contains(text), "{args:?}:\n{stderr}");
        }
    }

    // Without an adapter the same targets slice from the source.
    let (_, stderr, code) = fixture.run(&["slice", "spec/Loose.pi#Loose"]);
    assert_eq!(code, 0, "{stderr}");

    // A project without Belay has nothing to cite.
    let plain = slice_project("slice-adapter-plain");
    let (_, stderr, code) = plain.run(&["slice", "SaveButton", "--adapter", "claude-code"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("no Belay configuration"), "{stderr}");
}

/// The template names `piton init --list` reports.
fn templates(fixture: &Fixture) -> Vec<String> {
    let (stdout, stderr, code) = fixture.run(&["init", "--list"]);
    assert_eq!(code, 0, "{stderr}");
    // Templates are the indented lines, under their group's line.
    stdout
        .lines()
        .filter(|line| line.starts_with("  "))
        .map(|line| line.split_whitespace().next().expect("name").to_string())
        .collect()
}

#[test]
fn every_template_starts_a_project_that_checks_and_builds() {
    let fixture = Fixture::new("init-templates");
    let names = templates(&fixture);
    for expected in [
        "belay/application",
        "belay/minimal",
        "piton/library",
        "piton/minimal",
    ] {
        assert!(names.iter().any(|name| name == expected), "{names:?}");
    }

    for name in &names {
        let (stdout, stderr, code) = fixture.run(&["init", name, "--template", name]);
        assert_eq!(code, 0, "{name}: {stderr}");
        assert!(stdout.contains(&format!("{name}/piton.config.pi")), "{name}: {stdout}");
        assert!(
            !fixture.exists(&format!("{name}/.template")),
            "the description is not part of the project"
        );

        let (stdout, stderr, code) = fixture.run_in(name, &["check"]);
        assert_eq!(code, 0, "{name}: {stdout}{stderr}");
        assert!(!stderr.contains("warning"), "{name}: {stderr}");
        let (stdout, stderr, code) = fixture.run_in(name, &["format", "--check"]);
        assert_eq!(code, 0, "{name} is not canonically formatted: {stdout}{stderr}");
        let (stdout, stderr, code) = fixture.run_in(name, &["build"]);
        assert_eq!(code, 0, "{name}: {stdout}{stderr}");
    }

    assert!(fixture.exists("belay/application/.claude/skills/review-change/SKILL.md"));
    assert!(fixture.exists("belay/application/src/CLAUDE.md"));
    assert!(fixture.exists("belay/minimal/.claude/skills/explain/SKILL.md"));
    assert!(fixture.exists("piton/minimal/dist/index.json"));
}

#[test]
fn init_builds_a_belay_project_for_the_adapters_chosen() {
    let fixture = Fixture::new("init-adapters");
    for adapter in ["claude-code", "codex", "opencode"] {
        let (_, stderr, code) =
            fixture.run(&["init", adapter, "--template", "belay/application", "--adapter", adapter]);
        assert_eq!(code, 0, "{adapter}: {stderr}");
        let (stdout, stderr, code) = fixture.run_in(adapter, &["format", "--check"]);
        assert_eq!(code, 0, "{adapter}: {stdout}{stderr}");
        let (stdout, stderr, code) = fixture.run_in(adapter, &["build"]);
        assert_eq!(code, 0, "{adapter}: {stdout}{stderr}");
    }
    assert!(fixture.exists("codex/.agents/skills/review-change/SKILL.md"));
    assert!(
        !fixture.exists("codex/.claude"),
        "only the chosen adapter is built"
    );
    assert!(fixture.exists("opencode/.opencode/skills/review-change/SKILL.md"));
    assert!(fixture.exists("opencode/opencode.json"), "OpenCode is told to load src/AGENTS.md");
    assert!(!fixture.exists("codex/opencode.json"), "only the chosen adapter's files are written");
    assert!(!fixture.exists("codex/.adapters"));

    let (_, stderr, code) = fixture.run(&[
        "init",
        "several",
        "--template",
        "belay/application",
        "--adapter",
        "codex,claude-code",
        "-a",
        "codex",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("for claude-code, codex"), "{stderr}");
    let config = fixture.read("several/piton.config.pi");
    assert!(
        config.contains("import ClaudeCodeAdapter, CodexAdapter"),
        "{config}"
    );
    assert!(
        config.contains("- {ClaudeCodeAdapter}\n        - {CodexAdapter}\n"),
        "{config}"
    );
    let (stdout, stderr, code) = fixture.run_in("several", &["build"]);
    assert_eq!(code, 0, "{stdout}{stderr}");

    let (_, stderr, code) =
        fixture.run(&["init", "bad", "--template", "belay/minimal", "--adapter", "cursor"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("`cursor` is not an adapter"), "{stderr}");
    let (_, stderr, code) = fixture.run(&[
        "init",
        "plain",
        "--template",
        "piton/minimal",
        "--adapter",
        "codex",
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("doesn't use Belay"), "{stderr}");
    assert!(
        !fixture.exists("bad") && !fixture.exists("plain"),
        "nothing is written on an error"
    );
}

#[test]
fn init_never_overwrites_a_file() {
    let fixture = Fixture::new("init-existing");
    fixture.write("piton.config.pi", "// mine\n");

    let (stdout, stderr, code) = fixture.run(&["init", "--template", "piton/minimal"]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("nothing was written"), "{stderr}");
    assert!(stderr.contains("note: piton.config.pi"), "{stderr}");
    assert_eq!(fixture.read("piton.config.pi"), "// mine\n");
    assert!(!fixture.exists("spec"), "no file is written when any would be overwritten");
}

#[test]
fn init_needs_a_template_it_knows() {
    let fixture = Fixture::new("init-choose");

    // With no terminal to ask on, the template has to be named.
    let (_, stderr, code) = fixture.run(&["init"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("pick a template with --template"), "{stderr}");

    let (_, stderr, code) = fixture.run(&["init", "--template", "nope"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("`nope` is not a template; the templates are"), "{stderr}");

    // A group alone isn't enough without a terminal to pick from it.
    let (_, stderr, code) = fixture.run(&["init", "--template", "belay"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("`belay` is a group of templates"), "{stderr}");
    assert!(stderr.contains("`belay/application`, `belay/minimal`"), "{stderr}");
    assert!(!fixture.exists("piton.config.pi"));
}

#[test]
fn a_markdown_link_an_author_writes_is_passed_through() {
    // Piton has no link syntax of its own, so a Markdown link or image in
    // prose is text: Belay checks the links it generates from references,
    // never these.
    let fixture = slice_adapter_project("authored-links");
    fixture.write(
        "spec/Reference.pi",
        "export anchor HouseStyle:\n    \
         description: Every component keeps its props flat.\n    \
         naming: Spell names out.\n    \
         syntax:\n        \\\\\\\n        \
         ![A diagram of the parser](./images/parser.png)\n        \\\\\\\n    \
         guide: See [the guide](../docs/guide.md#start) before changing it.\n",
    );

    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let reference = fixture.read(".claude/reference/Reference.md");
    assert!(reference.contains("![A diagram of the parser](./images/parser.png)"), "{reference}");
    assert!(reference.contains("[the guide](../docs/guide.md#start)"), "{reference}");
}

#[test]
fn rebuilding_produces_identical_bytes() {
    let fixture = full_project(
        "determinism",
        "        - {ClaudeCodeAdapter}\n        - {CodexAdapter}",
    );
    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");

    let paths = [
        ".claude/skills/review-components/SKILL.md",
        ".claude/agents/reviewer.md",
        ".codex/agents/reviewer.toml",
        "src/components/button/CLAUDE.md",
        "src/components/button/AGENTS.md",
    ];
    let before: Vec<String> = paths.iter().map(|p| fixture.read(p)).collect();

    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    let after: Vec<String> = paths.iter().map(|p| fixture.read(p)).collect();
    assert_eq!(before, after, "a second build must produce the same bytes");
}

#[test]
fn compile_can_report_what_it_read() {
    let fixture = Fixture::new("compile-dependencies");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write("spec/base.pi", "export anchor Base:\n    kind: base\n");
    fixture.write(
        "spec/index.pi",
        "from ./base import Base\n\nexport anchor Thing:\n    uses: {Base}\n",
    );

    let (stdout, stderr, code) = fixture.run(&[
        "compile",
        "--renderer",
        "json",
        "--dependencies",
        "spec/index.pi",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"value\""), "{stdout}");
    // The imported file is a dependency even though the caller never named
    // it, and every path is absolute although the argument was relative.
    let base = fixture.dir.join("spec/base.pi");
    let index = fixture.dir.join("spec/index.pi");
    assert!(stdout.contains(&format!("\"{}\"", base.display())), "{stdout}");
    assert!(stdout.contains(&format!("\"{}\"", index.display())), "{stdout}");
    // A bundled package lives in the binary and cannot be watched.
    assert!(!stdout.contains("\"@piton"), "{stdout}");
}

#[test]
fn agent_prints_its_fluency_without_building_or_launching() {
    let fixture = full_project("print-fluency", "        - {CodexAdapter}\n");
    for args in [
        &["agent", "--print-fluency"][..],
        &["agent", "claude", "--print-fluency"],
    ] {
        let (stdout, stderr, code) = fixture.run(args);
        assert_eq!(code, 0, "{stderr}");
        assert!(
            stdout.contains("This project is written in Piton"),
            "{stdout}"
        );
        assert!(
            stdout.contains("AGENTS.md"),
            "follows the configured adapter:\n{stdout}"
        );
        assert!(
            stdout.contains("# Piton Fluency"),
            "carries the language reference:\n{stdout}"
        );
    }
    assert!(
        !fixture.exists("AGENTS.md"),
        "printing the prompt builds nothing"
    );
}

#[test]
fn build_output_references_point_into_the_output_tree() {
    let fixture = Fixture::new("dist-references");
    fixture.write(
        "piton.config.pi",
        "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
    );
    fixture.write("spec/components/Button.pi", "export anchor Button:\n    color: blue\n");
    fixture.write(
        "spec/index.pi",
        "use @piton/belay\n\nfrom @piton/belay import BELAY_PROJECT_ROOT\nfrom ./components/Button import Button\n\nexport anchor Index:\n    link: @{Button.color}\n    root: ${BELAY_PROJECT_ROOT}\n",
    );
    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let index = fixture.read("dist/index.json");
    assert!(index.contains("\"link\": \"./components/Button.json:Button.color\""), "{index}");
    assert!(!index.contains('\u{e000}'), "internal markers must not leak: {index}");
    assert!(index.contains("\"root\": \"..\""), "{index}");
    assert!(fixture.exists("dist/components/Button.json"));
}

#[test]
fn the_language_server_accepts_the_stdio_flag() {
    // Many editors' language clients (VS Code's among them) start a server
    // with `--stdio`. Rejecting it stops the server before it says a word.
    use std::io::{Read, Write};
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_piton"))
        .args(["lsp", "--stdio"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("piton lsp --stdio");
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}"#;
    let mut stdin = child.stdin.take().expect("stdin");
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("write");
    stdin.flush().expect("flush");
    let mut stdout = child.stdout.take().expect("stdout");
    let mut header = Vec::new();
    let mut byte = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        let read = stdout.read(&mut byte).expect("read");
        assert!(read > 0, "the server exited without answering");
        header.push(byte[0]);
    }
    let header = String::from_utf8_lossy(&header);
    let length: usize = header
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .and_then(|value| value.trim().parse().ok())
        .expect("a content length");
    let mut response = vec![0u8; length];
    stdout.read_exact(&mut response).expect("response");
    let _ = child.kill();
    let response = String::from_utf8_lossy(&response);
    assert!(response.contains("\"capabilities\""), "{response}");
}
