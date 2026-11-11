//! `piton test`, end to end, with a stand-in for the agentic tool.
//!
//! The tool is a shell script put first on PATH, so these run only where a
//! shell does. It answers by keyword and records what it was asked, which is
//! what lets a test check the request without a model in the loop.

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

/// A stand-in `claude`: it records its arguments under `calls/`, then gives
/// a verdict chosen by what the request says.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
dir="$(dirname "$0")/../calls"
mkdir -p "$dir"
printf '%s\n' "$@" > "$dir/$$"
case "$2" in
  *"has a name"*) echo 'PITON-VERDICT: {"pass": true, "reason": "App sets a name"}';;
  *"saves the document"*) echo 'PITON-VERDICT: {"pass": false, "reason": "nothing saves", "spec": {"pass": true, "reason": "The spec says Save saves."}, "code": {"pass": false, "reason": "No handler saves."}, "explanation": "The Save button has no click handler.", "fix": "Add a click handler to SaveButton in src/editor.ts."}';;
  *"halves split"*) echo 'PITON-VERDICT: {"pass": true, "reason": "close enough", "spec": {"pass": true, "reason": "The spec agrees."}, "code": {"pass": false, "reason": "The code does not."}, "explanation": "The code differs.", "fix": "Change the code."}';;
  *"crashes"*) echo 'boom' >&2; exit 3;;
  *) echo 'I am not sure.';;
esac
"#;

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("piton-testing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let fixture = Fixture { dir };
        fixture.write("bin/claude", FAKE_CLAUDE);
        let script = fixture.dir.join("bin/claude");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        fixture
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
    }

    /// Runs piton with only the stand-in tool and the system on PATH.
    fn run(&self, args: &[&str]) -> (String, String, i32) {
        let path = format!("{}:/usr/bin:/bin", self.dir.join("bin").display());
        let output = Command::new(binary())
            .args(args)
            .current_dir(&self.dir)
            .env("PATH", path)
            .env("NO_COLOR", "1")
            .output()
            .expect("run piton");
        (
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            output.status.code().unwrap_or(-1),
        )
    }

    /// The arguments of every call the stand-in recorded.
    fn calls(&self) -> Vec<Vec<String>> {
        let Ok(entries) = std::fs::read_dir(self.dir.join("calls")) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| {
                std::fs::read_to_string(entry.path())
                    .expect("call")
                    .lines()
                    .map(str::to_string)
                    .collect()
            })
            .collect()
    }
}

fn project(name: &str, testing: &str, tests: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fixture.write(
        "piton.config.pi",
        &format!("use @piton/config\nuse @piton/test\n\nexport piton-config App:\n    root: ./spec\n{testing}"),
    );
    fixture.write("spec/index.pi", "export anchor App:\n    name: app\n");
    fixture.write("spec/tests/App.pi", tests);
    fixture
}

const TESTS: &str = "use @piton/test

export test AppHasName:
    expects: The App anchor has a name
    against: spec

export test SaveWorks:
    expects: The Save button saves the document

test Unexported:
    expects: The App anchor has a name
";

fn request_of<'a>(calls: &'a [Vec<String>], needle: &str) -> &'a Vec<String> {
    calls
        .iter()
        .find(|call| call.iter().any(|line| line.contains(needle)))
        .unwrap_or_else(|| panic!("no call mentions {needle}: {calls:?}"))
}

#[test]
fn exported_tests_are_judged_and_failures_fail_the_run() {
    let fixture = project("judged", "", TESTS);
    let (stdout, stderr, code) = fixture.run(&["test"]);
    assert_eq!(code, 1, "{stdout}\n{stderr}");
    assert!(stdout.contains("App sets a name"), "{stdout}");
    assert!(stdout.contains("✗ FAIL   SaveWorks"), "{stdout}");
    assert!(stdout.contains("         spec/tests/App.pi\n"), "{stdout}");
    // A failure is explained beneath its reason, in ASD-STE100.
    assert!(stdout.contains("         nothing saves\n         │ The Save button has no click handler.\n"), "{stdout}");
    // What didn't pass is listed again at the end.
    assert!(stdout.contains("Not passing\n  ✗ FAIL   SaveWorks  spec/tests/App.pi (spec pass, code FAIL)"), "{stdout}");
    // Judged against both, it shows each side's result.
    assert!(stdout.contains("         spec ✓ pass  The spec says Save saves.\n         code ✗ FAIL  No handler saves.\n"), "{stdout}");
    // A failure comes with a prompt to fix it, whole and unwrapped, last. A
    // test judged against both fixes both.
    let prompts = stdout.split("Fix prompts\n").nth(1).unwrap_or_else(|| panic!("{stdout}"));
    assert!(prompts.contains("── SaveWorks ──"), "{prompts}");
    assert!(
        prompts.contains("\nFix the Piton specification and the code so that this statement is true in both:\n\nThe Save button saves the document\n"),
        "{prompts}"
    );
    assert!(prompts.contains("The test `SaveWorks` in `spec/tests/App.pi` examines the statement in the specification"), "{prompts}");
    assert!(
        prompts.contains("\nReason: nothing saves\nSpecification: the statement is true. The spec says Save saves.\nCode: the statement is not true. No handler saves.\nExplanation: The Save button has no click handler.\n"),
        "{prompts}"
    );
    assert!(prompts.contains("\nWhat to change:\nAdd a click handler to SaveButton in src/editor.ts.\n"), "{prompts}");
    assert!(prompts.contains("- Change the specification and the code. After the fix, they must agree.\n"), "{prompts}");
    assert!(prompts.contains("- Do not change the test.\n"), "{prompts}");
    assert!(prompts.ends_with("run `piton test SaveWorks --against both`.\n"), "{prompts}");
    // Only failures get one: a passing test doesn't.
    assert!(!prompts.contains("AppHasName"), "{prompts}");
    assert!(!stdout.contains("Unexported"), "{stdout}");
    assert!(stderr.contains("2 tests: 1 passed, 1 failed in 0:0"), "{stderr}");
    // Piped, each test says when it starts, rather than drawing a status
    // block, and each result says how long it took.
    assert!(stderr.contains("running AppHasName"), "{stderr}");
    assert!(stderr.contains("running SaveWorks"), "{stderr}");
    assert!(stdout.contains("✓ pass   AppHasName"), "{stdout}");
    // Unreached by the entry, the tests still ran: two calls, one each.
    assert_eq!(fixture.calls().len(), 2);
}

#[test]
fn the_tool_runs_headless_and_read_only() {
    let fixture = project("headless", "", TESTS);
    fixture.run(&["test", "AppHasName"]);
    let calls = fixture.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    let call = request_of(&calls, "has a name");
    assert_eq!(call[0], "-p");
    assert!(call.iter().any(|arg| arg == "--append-system-prompt"));
    assert!(call.iter().any(|arg| arg == "Edit,Write,NotebookEdit,Bash"));
    assert!(call.iter().any(|arg| arg.contains("PITON-VERDICT:")));
    assert!(call.iter().any(|arg| arg.contains("ASD-STE100")));
    assert!(call.iter().any(|arg| arg.contains("\"fix\": \"<only when pass is false>\"")));
}

#[test]
fn the_flag_beats_the_test_and_the_test_beats_the_config() {
    let fixture = project("precedence", "    plugins:\n        - {Tests}\n\ntest-config Tests:\n    against: code\n", TESTS);
    let (stdout, _, code) = fixture.run(&["test", "--list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("spec/tests/App.pi\n  AppHasName  against spec\n"), "{stdout}");
    assert!(stdout.contains("  SaveWorks  against code\n"), "{stdout}");
    // Listing runs nothing.
    assert!(fixture.calls().is_empty());

    let (stdout, _, _) = fixture.run(&["test", "--list", "--against", "both"]);
    assert!(stdout.contains("  AppHasName  against both\n"), "{stdout}");
    assert!(stdout.contains("  SaveWorks  against both\n"), "{stdout}");

    fixture.run(&["test", "SaveWorks"]);
    let calls = fixture.calls();
    let call = request_of(&calls, "saves the document");
    let request = call.join("\n");
    assert!(request.contains("the code only"), "{request}");
}

#[test]
fn without_a_verdict_a_test_is_an_error_not_a_failure() {
    let fixture = project(
        "no-verdict",
        "",
        "use @piton/test\n\nexport test Vague:\n    expects: Something vague\n\nexport test Crashes:\n    expects: The tool crashes\n",
    );
    let (stdout, stderr, code) = fixture.run(&["test", "--serial"]);
    assert_eq!(code, 1);
    assert!(stdout.contains("! error  Vague"), "{stdout}");
    assert!(stdout.contains("`claude` ended without a verdict line"), "{stdout}");
    assert!(stdout.contains("! error  Crashes"), "{stdout}");
    assert!(stdout.contains("`claude` exited with code 3: boom"), "{stdout}");
    assert!(stderr.contains("2 tests: 0 passed, 2 could not be judged in"), "{stderr}");
}

#[test]
fn targets_select_tests_by_name_or_file() {
    let fixture = project("targets", "", TESTS);
    let (stdout, _, code) = fixture.run(&["test", "--list", "spec/tests"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("AppHasName") && stdout.contains("SaveWorks"), "{stdout}");

    let (stdout, _, _) = fixture.run(&["test", "--list", "SaveWorks"]);
    assert!(!stdout.contains("AppHasName"), "{stdout}");

    let (_, stderr, code) = fixture.run(&["test", "Nothing"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("`Nothing` matches no test by name, file or directory"), "{stderr}");
}

#[test]
fn invalid_settings_stop_the_run() {
    let fixture = project(
        "invalid",
        "    plugins:\n        - {Tests}\n\ntest-config Tests:\n    agent: gpt\n    jobs: 0\n",
        TESTS,
    );
    let (_, stderr, code) = fixture.run(&["test"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("unknown-test-agent"), "{stderr}");
    assert!(stderr.contains("invalid-jobs"), "{stderr}");
    assert!(fixture.calls().is_empty());

    let fixture = project(
        "invalid-test",
        "",
        "use @piton/test\n\nexport test Odd:\n    expects: Anything\n    against: docs\n",
    );
    let (_, stderr, code) = fixture.run(&["test"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("is judged against `docs`"), "{stderr}");
}

#[test]
fn a_missing_tool_is_named() {
    let fixture = project("missing-tool", "", TESTS);
    let (_, stderr, code) = fixture.run(&["test", "--agent", "codex"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("`codex` is not on PATH"), "{stderr}");
}

#[test]
fn a_project_without_tests_passes() {
    let fixture = project("no-tests", "", "export anchor Nothing:\n    pass\n");
    let (_, stderr, code) = fixture.run(&["test"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("no tests found"), "{stderr}");
}

#[test]
fn tags_narrow_the_run() {
    let fixture = project(
        "tags",
        "",
        "use @piton/test

export test Saves:
    expects: Saving works
    tags: [editor, save]

export test Opens:
    expects: Opening works
    tags: [editor]

export test Slow:
    expects: The slow path works
    tags: [slow]

export test Untagged:
    expects: Something untagged
",
    );
    let listed = |args: &[&str]| -> Vec<String> {
        let mut all = vec!["test", "--list"];
        all.extend_from_slice(args);
        let (stdout, stderr, _) = fixture.run(&all);
        let _ = stderr;
        stdout
            .lines()
            .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
            .map(|line| line.trim_start().split(' ').next().unwrap_or("").to_string())
            .collect()
    };

    assert_eq!(listed(&[]), ["Opens", "Saves", "Slow", "Untagged"]);
    assert_eq!(listed(&["--tags=save or slow"]), ["Saves", "Slow"]);
    assert_eq!(listed(&["--tags", "editor"]), ["Opens", "Saves"]);
    assert_eq!(listed(&["--tags", "editor and save"]), ["Saves"]);
    assert_eq!(listed(&["--tags", "editor not save"]), ["Opens"]);
    // A leading not keeps untagged tests.
    assert_eq!(listed(&["--tags", "not editor"]), ["Slow", "Untagged"]);
    assert_eq!(listed(&["--tags", "((editor and save) not slow) or slow"]), ["Saves", "Slow"]);
    assert_eq!(listed(&["--tags", "!(editor || slow)"]), ["Untagged"]);
    // With a target, both have to select a test.
    assert_eq!(listed(&["Saves", "Slow", "--tags=slow"]), ["Slow"]);

    let (_, stderr, code) = fixture.run(&["test", "--tags", "(editor and"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("`--tags (editor and`: expected a tag, found the end"), "{stderr}");

    let (stdout, _, _) = fixture.run(&["test", "--list", "Saves"]);
    assert!(stdout.contains("  Saves  against both · editor, save"), "{stdout}");

    let (_, stderr, code) = fixture.run(&["test", "--tags=nothing"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("no tests match"), "{stderr}");
    assert!(fixture.calls().is_empty());
}

#[test]
fn a_fix_prompt_aims_at_what_the_test_examines() {
    let fixture = project("fix-scope", "", TESTS);
    let prompt = |against: &str| -> String {
        let (stdout, _, _) = fixture.run(&["test", "SaveWorks", "--against", against]);
        stdout.split("Fix prompts\n").nth(1).unwrap_or_else(|| panic!("{stdout}")).to_string()
    };

    let code = prompt("code");
    assert!(code.contains("\nFix the code so that this statement is true:\n"), "{code}");
    assert!(code.contains("- Change only the code. Do not change the specification.\n"), "{code}");
    assert!(!code.contains("piton build"), "{code}");
    assert!(code.ends_with("run `piton test SaveWorks --against code`.\n"), "{code}");

    let spec = prompt("spec");
    assert!(spec.contains("\nFix the Piton specification so that this statement is true:\n"), "{spec}");
    assert!(spec.contains("examines the statement in the specification (the `.pi` files under `spec`)."), "{spec}");
    assert!(spec.contains("- Change only the specification. Do not change the code.\n"), "{spec}");
    assert!(spec.contains("Then run `piton check` and `piton build`."), "{spec}");
    assert!(spec.ends_with("run `piton test SaveWorks --against spec`.\n"), "{spec}");

    // The tool is asked for a fix aimed the same way.
    let calls = fixture.calls();
    assert!(calls.iter().any(|call| call.iter().any(|arg| arg.contains("Do not change the specification."))));
    assert!(calls.iter().any(|call| call.iter().any(|arg| arg.contains("Do not change the code."))));
}

#[test]
fn against_both_a_test_fails_when_one_side_fails() {
    let fixture = project(
        "sides",
        "",
        "use @piton/test\n\nexport test Split:\n    expects: The two halves split apart\n\nexport test SpecOnly:\n    expects: The two halves split apart\n    against: spec\n",
    );
    let (stdout, stderr, code) = fixture.run(&["test", "Split"]);
    // The tool said pass, but the code doesn't hold: the test fails, and the
    // two results show why.
    assert_eq!(code, 1, "{stdout}\n{stderr}");
    assert!(stdout.contains("✗ FAIL   Split"), "{stdout}");
    assert!(stdout.contains("spec ✓ pass  The spec agrees.\n         code ✗ FAIL  The code does not.\n"), "{stdout}");
    assert!(stdout.contains("Fix prompts"), "{stdout}");

    // Against one side, the two results mean nothing and the tool's pass
    // stands.
    let (stdout, _, code) = fixture.run(&["test", "SpecOnly"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(!stdout.contains("spec ✓ pass"), "{stdout}");
}
