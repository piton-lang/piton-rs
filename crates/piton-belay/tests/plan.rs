//! Checks the output plan against the Belay specification, one rule at a
//! time, on small sandbox projects.

use std::path::PathBuf;

use piton_compile::{config, Compilation, Framework};

struct Sandbox {
    dir: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join(format!(
            "piton-plan-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).expect("temp dir");
        Sandbox { dir }
    }

    fn write(&self, relative: &str, contents: &str) -> &Sandbox {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, contents).expect("write");
        self
    }

    fn mkdir(&self, relative: &str) -> &Sandbox {
        std::fs::create_dir_all(self.dir.join(relative)).expect("mkdir");
        self
    }

    /// Writes a configuration with the given adapters and extra belay-config
    /// lines.
    fn configure(&self, adapters: &[&str], extra: &str) -> &Sandbox {
        let imports = adapters.join(", ");
        let list: String = adapters
            .iter()
            .map(|adapter| format!("        - {{{adapter}}}\n"))
            .collect();
        self.write(
            "piton.config.pi",
            &format!(
                "use @piton/config\nuse @piton/belay\n\nfrom @piton/belay import {imports}\n\nexport piton-config Sandbox:\n    root: ./spec\n    entry: ./spec/index.pi\n\n    frameworks:\n        - {{SandboxBelay}}\n\nbelay-config SandboxBelay:\n    codeRoot: ./src\n    shapeRoot: ./spec/shape\n{extra}\n    adapters:\n{list}"
            ),
        )
    }

    fn plan(&self) -> Built {
        let (project, diagnostics) = config::load(&self.dir, None);
        assert!(
            !diagnostics.has_errors(),
            "configuration did not load: {:?}",
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
        let belay = project
            .frameworks
            .iter()
            .find_map(|framework| match framework {
                Framework::Belay(config) => Some(config.clone()),
            })
            .expect("belay configured");
        let compilation = Compilation::build(project);
        assert!(
            !compilation.has_errors(),
            "sandbox did not compile: {:?}",
            compilation
                .diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
        let plan = piton_belay::plan(&compilation, &belay);
        Built { plan }
    }
}

struct Built {
    plan: piton_belay::Plan,
}

impl Built {
    fn file(&self, path: &str) -> &str {
        &self
            .plan
            .files
            .iter()
            .find(|file| file.path == PathBuf::from(path))
            .unwrap_or_else(|| panic!("{path} was not planned; planned: {:?}", self.paths()))
            .contents
    }

    fn has(&self, path: &str) -> bool {
        self.plan.files.iter().any(|file| file.path == PathBuf::from(path))
    }

    fn paths(&self) -> Vec<String> {
        self.plan
            .files
            .iter()
            .map(|file| file.path.to_string_lossy().to_string())
            .collect()
    }

    fn codes(&self) -> Vec<String> {
        self.plan.diagnostics.iter().map(|d| d.code.clone()).collect()
    }

    fn diagnostic(&self, code: &str) -> &piton_core::Diagnostic {
        self.plan
            .diagnostics
            .iter()
            .find(|d| d.code == code)
            .unwrap_or_else(|| panic!("no `{code}` diagnostic; got {:#?}", self.plan.diagnostics))
    }

    fn assert_clean(&self) {
        let errors: Vec<String> = self
            .plan
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
    }
}

const CLAUDE: &[&str] = &["ClaudeCodeAdapter"];

#[test]
fn only_the_entry_exports_and_what_they_reference_are_emitted() {
    let sandbox = Sandbox::new("emission");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nskill Parent:\n    useWhen: never directly\n    prompt: The parent.\n\nskill Embedded:\n    useWhen: embedded\n    prompt: Embedded.\n\nexport skill Child extends Parent:\n    useWhen: asked\n\nexport anchor Top:\n    embeds: {Embedded}\n",
    );
    let built = sandbox.plan();
    built.assert_clean();
    assert!(built.has(".claude/skills/child/SKILL.md"));
    assert!(!built.has(".claude/skills/parent/SKILL.md"), "{:?}", built.paths());
    assert!(!built.has(".claude/skills/embedded/SKILL.md"), "{:?}", built.paths());
    assert!(built.file(".claude/reference/index.md").contains("# Top"));
}

#[test]
fn a_reference_to_an_abstract_or_package_anchor_is_an_error() {
    let sandbox = Sandbox::new("abstract-ref");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nfrom @piton/belay import Skill\n\nabstract anchor Shape:\n    size:: number\n\nexport skill Uses:\n    useWhen: asked\n    prompt: See @{Shape} and @{Skill}.\n",
    );
    let built = sandbox.plan();
    let errors: Vec<&piton_core::Diagnostic> = built
        .plan
        .diagnostics
        .iter()
        .filter(|d| d.code == "unrepresentable-reference")
        .collect();
    assert_eq!(errors.len(), 2, "{:#?}", built.plan.diagnostics);
    assert!(errors.iter().all(|d| d.span != piton_core::Span::default()));
    assert!(!built.file(".claude/skills/uses/SKILL.md").contains("]("));
}

#[test]
fn references_link_to_the_heading_in_the_module_file() {
    let sandbox = Sandbox::new("links");
    sandbox
        .configure(CLAUDE, "")
        .write(
            "spec/a/Ref.pi",
            "export anchor Ref:\n    description: The first.\n\nexport anchor Other:\n    description: A neighbour.\n    color: red\n",
        )
        .write("spec/b/Ref.pi", "export anchor Ref:\n    description: The second.\n")
        .write(
            "spec/b/User.pi",
            "from ./Ref import Ref\n\nexport anchor User:\n    see: Also @{Ref}.\n",
        )
        .write(
            "spec/index.pi",
            "use @piton/belay\n\nfrom ./a/Ref import Ref, Other\nfrom ./b/User import User\n\nexport skill Links:\n    useWhen: asked\n    prompt: Read @{Ref}, @{User}, and @{Other.color}.\n",
        );
    let built = sandbox.plan();
    built.assert_clean();
    let skill = built.file(".claude/skills/links/SKILL.md");
    assert!(skill.contains("[Ref](../../reference/a/Ref.md#ref)"), "{skill}");
    assert!(skill.contains("[User](../../reference/b/User.md#user)"), "{skill}");
    assert!(
        built.file(".claude/reference/b/User.md").contains("[Ref](./Ref.md#ref)"),
        "two anchors named Ref land in two files"
    );
    assert!(
        skill.contains("[Other.color](../../reference/a/Ref.md#color)"),
        "{skill}"
    );
    let first = built.file(".claude/reference/a/Ref.md");
    assert!(first.contains("# Ref\n") && first.contains("# Other\n"), "{first}");
    assert!(built.file(".claude/reference/b/Ref.md").contains("The second."));
    assert!(!first.contains("Links in this document"), "{first}");
}

#[test]
fn repeated_headings_in_one_file_get_distinct_fragments() {
    let sandbox = Sandbox::new("fragments");
    sandbox
        .configure(CLAUDE, "")
        .write(
            "spec/Pair.pi",
            "export anchor First:\n    description: One.\n\nexport anchor Second:\n    description: Two.\n",
        )
        .write(
            "spec/index.pi",
            "use @piton/belay\n\nfrom ./Pair import First, Second\n\nexport skill Pairs:\n    useWhen: asked\n    prompt: Compare @{First.description} with @{Second.description}.\n",
        );
    let built = sandbox.plan();
    built.assert_clean();
    let skill = built.file(".claude/skills/pairs/SKILL.md");
    assert!(skill.contains("Pair.md#description)"), "{skill}");
    assert!(skill.contains("Pair.md#description-1)"), "{skill}");
}

#[test]
fn descriptions_are_required_where_the_target_needs_them_and_omitted_otherwise() {
    let sandbox = Sandbox::new("descriptions");
    sandbox.configure(&["ClaudeCodeAdapter", "CodexAdapter"], "    crossDiscovery: separate\n").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport skill Quiet:\n    useWhen: asked\n    prompt: Hi.\n\nexport command Go:\n    prompt: Go.\n",
    );
    let built = sandbox.plan();
    let command = built.file(".claude/skills/x-go/SKILL.md");
    assert!(!command.contains("description"), "{command}");
    let skill = built.file(".claude/skills/quiet/SKILL.md");
    assert!(skill.contains("description: Use when asked\n"), "{skill}");
    let missing: Vec<&str> = built
        .plan
        .diagnostics
        .iter()
        .filter(|d| d.code == "missing-description")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(missing.len(), 2, "Codex needs both: {missing:#?}");
    assert!(missing.iter().all(|m| m.contains("CodexAdapter")), "{missing:#?}");
}

#[test]
fn discovery_descriptions_join_with_a_single_space() {
    let sandbox = Sandbox::new("discovery");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport skill Tidy:\n    description: Tidies things\n    useWhen: asked\n",
    );
    let built = sandbox.plan();
    assert!(
        built
            .file(".claude/skills/tidy/SKILL.md")
            .contains("description: Tidies things Use when asked\n")
    );
}

#[test]
fn instructions_keep_their_description_and_nest_their_extras() {
    let sandbox = Sandbox::new("instruction");
    sandbox.configure(CLAUDE, "").mkdir("src/button").write(
        "spec/shape/button/Button.pi",
        "use @piton/belay\n\nexport instruction ButtonShape:\n    description: How the button is built.\n    prompt: Keep it small.\n    notes: Nothing else.\n",
    ).write("spec/index.pi", "from ./shape/button/Button export *\n");
    let built = sandbox.plan();
    built.assert_clean();
    assert_eq!(
        built.file("src/button/CLAUDE.md"),
        "# Button Shape\n\nHow the button is built.\n\nKeep it small.\n\n## Notes\n\nNothing else.\n"
    );
    assert!(built.has(".claude/reference/shape/button/Button.md"));
}

#[test]
fn instructions_outside_the_shape_root_are_not_specified() {
    let sandbox = Sandbox::new("unscoped");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport instruction Loose:\n    prompt: Anywhere.\n",
    );
    let built = sandbox.plan();
    let diagnostic = built.diagnostic("instruction-placement-unspecified");
    assert!(diagnostic.is_error());
    assert!(!built.has("src/CLAUDE.md"));
}

#[test]
fn unsupported_and_invalid_native_options_fail() {
    let sandbox = Sandbox::new("options");
    sandbox.configure(&["OpenCodeAdapter"], "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport agent Reviewer:\n    description: Reviews\n    role: careful reviewer\n    mode: sometimes\n    model: fast\n    tools:\n        - read\n",
    );
    let built = sandbox.plan();
    let unsupported = built.diagnostic("unsupported-option");
    assert!(unsupported.is_error());
    assert!(unsupported.message.contains("OpenCodeAdapter"), "{}", unsupported.message);
    assert_ne!(unsupported.span, piton_core::Span::default());
    let invalid: Vec<&str> = built
        .plan
        .diagnostics
        .iter()
        .filter(|d| d.code == "invalid-option")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(invalid.len(), 2, "mode and model: {invalid:#?}");
}

#[test]
fn agents_begin_with_the_role_introduction() {
    let sandbox = Sandbox::new("agent");
    sandbox.configure(&["OpenCodeAdapter"], "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport agent Reviewer:\n    description: Reviews\n    role: careful reviewer\n    prompt: Look closely.\n    model: anthropic/claude-sonnet-4-5\n    permission:\n        edit: deny\n",
    );
    let built = sandbox.plan();
    built.assert_clean();
    assert_eq!(
        built.file(".opencode/agents/reviewer.md"),
        "---\ndescription: Reviews\nmode: subagent\nmodel: anthropic/claude-sonnet-4-5\npermission:\n  edit: deny\n---\n\nYou are a careful reviewer\n\nLook closely.\n"
    );
}

#[test]
fn constructs_normalizing_to_one_identity_collide() {
    let sandbox = Sandbox::new("collision");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport skill X_Release:\n    useWhen: asked\n    prompt: Same.\n\nexport command Release:\n    prompt: Same.\n",
    );
    let built = sandbox.plan();
    assert!(built.diagnostic("name-collision").is_error());
}

#[test]
fn opencode_nested_guidance_needs_explicit_configuration() {
    let sandbox = Sandbox::new("opencode-scope");
    sandbox.configure(&["OpenCodeAdapter"], "").mkdir("src/button").write(
        "spec/shape/button/Button.pi",
        "use @piton/belay\n\nexport instruction ButtonShape:\n    prompt: Keep it small.\n",
    ).write("spec/index.pi", "from ./shape/button/Button export *\n");
    let built = sandbox.plan();
    assert!(built.diagnostic("unguaranteed-instruction-scope").is_error());

    sandbox.write("opencode.json", "{\n  \"instructions\": [\"src/**/AGENTS.md\"]\n}\n");
    sandbox.plan().assert_clean();
}

#[test]
fn codex_reports_over_budget_and_shadowed_guidance() {
    let sandbox = Sandbox::new("codex-scope");
    sandbox.configure(&["CodexAdapter"], "    instructionByteLimit: 10\n").mkdir("src/button").write(
        "spec/shape/button/Button.pi",
        "use @piton/belay\n\nexport instruction ButtonShape:\n    prompt: Keep it small, and keep it simple.\n",
    ).write("spec/index.pi", "from ./shape/button/Button export *\n");
    let built = sandbox.plan();
    assert!(built.diagnostic("instruction-over-budget").is_error());

    sandbox.configure(&["CodexAdapter"], "").write("src/button/AGENTS.override.md", "local\n");
    let built = sandbox.plan();
    assert!(built.diagnostic("shadowed-instruction").is_error());
    assert!(!built.codes().contains(&"instruction-over-budget".to_string()));
}

#[test]
fn cross_discovery_that_changes_activation_needs_a_choice() {
    let sandbox = Sandbox::new("cross");
    sandbox.configure(&["ClaudeCodeAdapter", "OpenCodeAdapter"], "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport command Release:\n    description: Cut a release\n    prompt: Go.\n",
    );
    let built = sandbox.plan();
    assert!(built.diagnostic("cross-discovery-activation").is_error());

    sandbox.configure(&["ClaudeCodeAdapter", "OpenCodeAdapter"], "    crossDiscovery: separate\n");
    let built = sandbox.plan();
    assert!(!built.codes().contains(&"cross-discovery-activation".to_string()));
    built.assert_clean();
}

#[test]
fn a_command_skill_opencode_is_told_to_hide_needs_no_choice() {
    let sandbox = Sandbox::new("cross-permission");
    sandbox.configure(&["ClaudeCodeAdapter", "OpenCodeAdapter"], "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport command Release:\n    description: Cut a release\n    prompt: Go.\n",
    );
    // A later pattern decides over an earlier one.
    sandbox.write(
        "opencode.json",
        "{\n  \"permission\": {\"skill\": {\"*\": \"allow\", \"x-*\": \"deny\"}}\n}\n",
    );
    let built = sandbox.plan();
    built.assert_clean();
    assert!(!built.codes().contains(&"cross-target-discovery".to_string()), "{:?}", built.codes());

    sandbox.write(
        "opencode.json",
        "{\n  \"permission\": {\"skill\": {\"x-*\": \"deny\", \"*\": \"allow\"}}\n}\n",
    );
    assert!(sandbox.plan().diagnostic("cross-discovery-activation").is_error());
}

#[test]
fn shared_projects_write_one_reference_tree_and_one_agents_md() {
    let sandbox = Sandbox::new("shared");
    sandbox
        .configure(&["CodexAdapter", "OpenCodeAdapter"], "")
        .write("opencode.json", "{\n  \"instructions\": [\"src/AGENTS.md\"]\n}\n")
        .write("spec/Style.pi", "export anchor Style:\n    rule: Flat props.\n")
        .write(
            "spec/shape/Source.pi",
            "use @piton/belay\n\nfrom ../Style import Style\n\nexport instruction Source:\n    prompt: Follow @{Style}.\n",
        )
        .write(
            "spec/index.pi",
            "use @piton/belay\n\nfrom ./Style import Style\nfrom ./shape/Source export *\n\nexport skill Tidy:\n    description: Tidies.\n    useWhen: asked\n    prompt: Follow @{Style}.\n",
        );
    let built = sandbox.plan();
    built.assert_clean();
    // The first adapter listed owns the one reference tree.
    assert!(built.has(".codex/reference/Style.md"));
    assert!(!built.has(".opencode/reference/Style.md"), "{:?}", built.paths());
    // Both tools read the one AGENTS.md, which links into that tree.
    assert!(built.file("src/AGENTS.md").contains("../.codex/reference/Style.md"));
    // OpenCode discovers the Codex skill, so it writes no copy of its own.
    assert!(built.has(".agents/skills/tidy/SKILL.md"));
    assert!(!built.has(".opencode/skills/tidy/SKILL.md"), "{:?}", built.paths());

    // Deployed apart, each tool gets its own tree and they can no longer
    // share AGENTS.md.
    sandbox.configure(&["CodexAdapter", "OpenCodeAdapter"], "    crossDiscovery: separate\n");
    let built = sandbox.plan();
    assert!(built.has(".opencode/reference/Style.md"));
    assert!(built.has(".opencode/skills/tidy/SKILL.md"));
    assert!(built.diagnostic("output-collision").is_error());
}

#[test]
fn a_skill_opencode_finds_twice_is_reported_once_per_skill() {
    let sandbox = Sandbox::new("twice");
    sandbox
        .configure(&["ClaudeCodeAdapter", "CodexAdapter", "OpenCodeAdapter"], "")
        .write(
            "spec/index.pi",
            "use @piton/belay\n\nexport skill Tidy:\n    description: Tidies.\n    useWhen: asked\n",
        );
    let built = sandbox.plan();
    built.assert_clean();
    let warning = built.diagnostic("cross-target-discovery");
    assert!(warning.message.contains("`.agents/skills/tidy` and `.claude/skills/tidy`"), "{}", warning.message);
    assert!(!built.has(".opencode/skills/tidy/SKILL.md"));
}

#[test]
fn the_target_versions_are_recorded() {
    let sandbox = Sandbox::new("record");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport skill Tidy:\n    useWhen: asked\n",
    );
    let built = sandbox.plan();
    // The manifest records, for each target, when its documentation was
    // last checked.
    let manifest = piton_belay::manifest_document(&built.plan);
    assert!(manifest.contains("\"targets\""), "{manifest}");
    assert!(manifest.contains("\"claude-code\": \"2026-09-21\""), "{manifest}");
}

#[test]
fn an_adapter_anchor_can_move_the_reference_root() {
    let sandbox = Sandbox::new("custom-adapter");
    sandbox
        .write(
            "piton.config.pi",
            "use @piton/config\nuse @piton/belay\n\nfrom @piton/belay import ClaudeCodeAdapter\n\nexport piton-config Sandbox:\n    root: ./spec\n    entry: ./spec/index.pi\n\n    frameworks:\n        - {SandboxBelay}\n\nbelay-config SandboxBelay:\n    codeRoot: ./src\n    adapters:\n        - {Docs}\n\nanchor Docs extends ClaudeCodeAdapter:\n    referenceRoot: .claude/docs\n",
        )
        .write("spec/Style.pi", "export anchor Style:\n    rule: Flat props.\n")
        .write(
            "spec/index.pi",
            "use @piton/belay\n\nfrom ./Style import Style\n\nexport skill Tidy:\n    useWhen: asked\n    prompt: Follow @{Style}.\n",
        );
    let built = sandbox.plan();
    built.assert_clean();
    assert!(built.file(".claude/skills/tidy/SKILL.md").contains("../../docs/Style.md#style"));
}

#[test]
fn existing_files_the_build_does_not_own_are_not_overwritten() {
    let sandbox = Sandbox::new("ownership");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport skill Tidy:\n    useWhen: asked\n",
    );
    sandbox.write(".claude/skills/tidy/SKILL.md", "hand written\n");
    let built = sandbox.plan();
    assert!(built.diagnostic("unowned-output").is_error());
    assert!(piton_belay::write(&built.plan, &sandbox.dir).is_err());
    assert_eq!(
        std::fs::read_to_string(sandbox.dir.join(".claude/skills/tidy/SKILL.md")).unwrap(),
        "hand written\n"
    );

    // Once the manifest records it, the build owns it.
    sandbox.write(
        piton_belay::MANIFEST,
        "{\n  \"generated\": [\n    \".claude/skills/tidy/SKILL.md\"\n  ]\n}\n",
    );
    let built = sandbox.plan();
    built.assert_clean();
    piton_belay::write(&built.plan, &sandbox.dir).expect("owned files are written");
    let manifest = std::fs::read_to_string(sandbox.dir.join(piton_belay::MANIFEST)).unwrap();
    assert!(manifest.contains("\"targets\""), "{manifest}");
}

#[test]
fn cleanup_only_removes_recorded_files() {
    let sandbox = Sandbox::new("cleanup");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport skill Tidy:\n    useWhen: asked\n",
    );
    sandbox.write(".claude/skills/old/SKILL.md", "stale\n");
    sandbox.write(".claude/skills/mine/SKILL.md", "user\n");
    let built = sandbox.plan();
    let removed = piton_belay::clean_stale(
        &[
            ".claude/skills/old/SKILL.md".to_string(),
            "../escape.md".to_string(),
        ],
        &built.plan,
        &sandbox.dir,
    );
    assert_eq!(removed, vec![PathBuf::from(".claude/skills/old/SKILL.md")]);
    assert!(!sandbox.dir.join(".claude/skills/old").exists(), "empty directory pruned");
    assert!(sandbox.dir.join(".claude/skills/mine/SKILL.md").exists());
}

#[test]
fn references_in_metadata_are_errors() {
    let sandbox = Sandbox::new("metadata-ref");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport anchor Style:\n    rule: Flat.\n\nexport skill Tidy:\n    description: Applies @{Style}\n    useWhen: asked\n",
    );
    let built = sandbox.plan();
    assert!(built.diagnostic("unrepresentable-reference").is_error());
}

#[test]
fn a_reference_to_a_construct_links_to_its_own_output() {
    let sandbox = Sandbox::new("construct-link");
    sandbox.configure(CLAUDE, "").write(
        "spec/index.pi",
        "use @piton/belay\n\nexport skill Tidy:\n    useWhen: asked\n\nexport skill Review:\n    useWhen: reviewing\n    prompt: Run @{Tidy} first.\n",
    );
    let built = sandbox.plan();
    built.assert_clean();
    let review = built.file(".claude/skills/review/SKILL.md");
    assert!(review.contains("[Tidy](../tidy/SKILL.md)"), "{review}");
    assert!(
        !built.has(".claude/reference/index.md"),
        "a construct is not copied into the reference directory"
    );
}
