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
        let output = Command::new(binary())
            .args(args)
            .current_dir(&self.dir)
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
    fixture.write("src/components/button/Button.ts", "export const Button = 1;\n");

    fixture.write(
        "piton.config.pi",
        &format!(
            "use @piton/config\nuse @piton/belay\n\nfrom @piton/belay import\n    ClaudeAdapter,\n    CodexAdapter,\n    OpenCodeAdapter\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n\n    frameworks:\n        - {{Belay}}\n\nbelay-config Belay:\n    codeRoot: ./src\n    shapeRoot: ./spec/shape\n\n    adapters:\n{adapters}\n"
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
fn a_full_project_builds_every_artifact() {
    let fixture = full_project("full", "        - {ClaudeAdapter}");
    let (stdout, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "stdout: {stdout}\nstderr: {stderr}");

    // Skill.
    let skill = fixture.read(".claude/skills/review-components/SKILL.md");
    assert!(skill.starts_with("---\nname: review-components\n"), "{skill}");
    assert!(
        skill.contains("description: Review a component against the house style. Use when reviewing or writing a component"),
        "{skill}"
    );
    assert!(skill.contains("# Checklist"), "additional properties serialize after the prompt:\n{skill}");
    assert!(skill.contains("- Props are flat."), "{skill}");

    // A reference in a prompt becomes a link to a generated file.
    assert!(skill.contains("](../../reference/HouseStyle.md)"), "{skill}");
    assert!(fixture.exists(".claude/reference/HouseStyle.md"));

    // Command: an x-prefixed skill with automatic invocation disabled.
    let command = fixture.read(".claude/skills/x-release/SKILL.md");
    assert!(command.contains("name: x-release"), "{command}");
    assert!(command.contains("disable-model-invocation: true"), "{command}");
    assert!(!fixture.exists(".claude/commands/x-release.md"), "one representation only");

    // Agent: role introduction, then prompt, with explicit native options only.
    let agent = fixture.read(".claude/agents/reviewer.md");
    assert!(agent.contains("name: reviewer"), "{agent}");
    assert!(agent.contains("model: fast"), "{agent}");
    assert!(agent.contains("You are a careful reviewer"), "{agent}");
    assert!(!agent.contains("tools:"), "absent options stay absent:\n{agent}");

    // Instruction: placed at the matching code scope, not at the shape path.
    let instruction = fixture.read("src/components/button/CLAUDE.md");
    assert!(instruction.contains("# Button Shape"), "{instruction}");
    assert!(instruction.contains("label and an onPress handler"), "{instruction}");
    // And preserved in the compiled shape tree.
    assert!(fixture.exists(".claude/reference/shape/components/button/ButtonShape.md"));
}

#[test]
fn an_instruction_falls_back_to_the_nearest_existing_scope() {
    let fixture = full_project("fallback", "        - {ClaudeAdapter}");
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
    assert!(fixture.exists(".claude/reference/shape/components/missing/GhostShape.md"));
}

#[test]
fn every_adapter_emits_its_own_native_form() {
    let fixture = full_project(
        "adapters",
        "        - {ClaudeAdapter}\n        - {CodexAdapter}\n        - {OpenCodeAdapter}",
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
    assert!(toml.contains("model = \"fast\""), "model is supported: {toml}");

    // OpenCode: commands are native and agent mode is explicit.
    let command = fixture.read(".opencode/commands/x-release.md");
    assert!(command.contains("description: Cut a release"), "{command}");
    assert!(!command.contains("name:"), "identity comes from the filename:\n{command}");
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

    let (json, stderr, code) = fixture.run(&["compile", "data.pi", "--adapter", "json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(json.contains("\"count\": 3"), "{json}");
    assert!(json.contains("\"ready\": true"), "{json}");
    assert!(json.contains("\"missing\": null"), "{json}");
    assert!(json.contains("\"tags\": [\n"), "{json}");

    let (yaml, _, code) = fixture.run(&["compile", "data.pi", "--adapter", "yaml"]);
    assert_eq!(code, 0);
    assert!(yaml.contains("name: Widget"), "{yaml}");
    assert!(yaml.contains("count: 3"), "{yaml}");

    let (markdown, _, code) = fixture.run(&["compile", "data.pi", "--adapter", "markdown"]);
    assert_eq!(code, 0);
    assert!(markdown.contains("# Thing"), "{markdown}");
    assert!(markdown.contains("## Name"), "{markdown}");
    assert!(markdown.contains("## Count"), "{markdown}");
}

#[test]
fn compile_writes_next_to_the_input_when_asked() {
    let fixture = Fixture::new("compile-write");
    fixture.write("data.pi", "export anchor Thing:\n    name: Widget\n");
    let (_, stderr, code) = fixture.run(&["compile", "data.pi", "--adapter", "yaml", "--write"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(fixture.read("data.yaml").contains("name: Widget"));
}

#[test]
fn a_glob_without_write_is_refused() {
    let fixture = Fixture::new("glob");
    fixture.write("a.pi", "export anchor A:\n    x: 1\n");
    fixture.write("b.pi", "export anchor B:\n    x: 2\n");
    let (_, stderr, code) = fixture.run(&["compile", "*.pi", "--adapter", "json"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("needs --write"), "{stderr}");

    let (_, stderr, code) = fixture.run(&["compile", "*.pi", "--adapter", "json", "--write"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(fixture.exists("a.json") && fixture.exists("b.json"));
}

#[test]
fn format_normalizes_and_check_reports() {
    let fixture = Fixture::new("format");
    fixture.write("messy.pi", "from ./x import C, A, B\nanchor A:\n\tvalue: 1 //tight\n");
    fixture.write("x.pi", "export anchor A:\n    v: 1\nexport anchor B:\n    v: 2\nexport anchor C:\n    v: 3\n");

    let (_, stderr, code) = fixture.run(&["format", "messy.pi", "--check"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("unformatted"), "{stderr}");

    let (_, _, code) = fixture.run(&["format", "messy.pi"]);
    assert_eq!(code, 0);
    let formatted = fixture.read("messy.pi");
    assert_eq!(
        formatted,
        "from ./x import\n    A,\n    B,\n    C\nanchor A:\n    value: 1 // tight\n"
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
    let fixture = full_project("reach", "        - {ClaudeAdapter}");
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
fn build_cleans_up_only_what_it_generated() {
    let fixture = full_project("cleanup", "        - {ClaudeAdapter}");
    let (_, stderr, code) = fixture.run(&["build"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(fixture.exists(".claude/skills/x-release/SKILL.md"));

    // A file the user wrote inside an output directory must survive.
    fixture.write(".claude/skills/mine/SKILL.md", "---\nname: mine\n---\n\nhand written\n");

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
    let fixture = full_project("dry", "        - {ClaudeAdapter}");
    let (stdout, stderr, code) = fixture.run(&["build", "--dry-run"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains(".claude/skills/review-components/SKILL.md"), "{stdout}");
    assert!(!fixture.exists(".claude/skills/review-components/SKILL.md"));
}

#[test]
fn an_unknown_adapter_is_rejected() {
    let fixture = Fixture::new("badadapter");
    fixture.write("a.pi", "export anchor A:\n    x: 1\n");
    let (_, stderr, code) = fixture.run(&["compile", "a.pi", "--adapter", "toml"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("unknown adapter `toml`"), "{stderr}");
}

#[test]
fn help_lists_every_documented_command() {
    let output = Command::new(binary()).arg("--help").output().expect("help");
    let text = String::from_utf8_lossy(&output.stdout);
    for command in ["agent", "build", "check", "compile", "format", "loc", "lsp", "reach"] {
        assert!(text.contains(command), "`{command}` missing from help:\n{text}");
    }
    let _ = Path::new(".");
}

#[test]
fn rebuilding_produces_identical_bytes() {
    let fixture = full_project("determinism", "        - {ClaudeAdapter}\n        - {CodexAdapter}");
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
