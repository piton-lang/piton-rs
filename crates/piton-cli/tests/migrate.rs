//! The migrate plugin, end to end: a project written for an older Piton is
//! noticed before any command, and `piton migrate` brings it up to date.
//!
//! These run piped, where nothing is asked: the findings are reported, and
//! only `piton migrate` changes files.

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
    fn new(name: &str, config: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("piton-migrate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("spec")).expect("dirs");
        std::fs::write(dir.join("piton.config.pi"), config).expect("config");
        std::fs::write(dir.join("spec/index.pi"), "export anchor App:\n    name: app\n").expect("spec");
        Fixture { dir }
    }

    fn config(&self) -> String {
        std::fs::read_to_string(self.dir.join("piton.config.pi")).expect("config")
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

const OLD: &str = "use @piton/belay
use @piton/config

from @piton/belay import ClaudeCodeAdapter

export piton-config App:
    root: ./spec

    frameworks:
        - {Belay}

belay-config Belay:
    codeRoot: ./src
    adapters:
        - {ClaudeCodeAdapter}
";

#[test]
fn an_old_project_is_noticed_before_any_command_and_left_alone() {
    let fixture = Fixture::new("noticed", OLD);
    let (_, stderr, code) = fixture.run(&["check"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("piton.config.pi lists frameworks, which is now plugins"), "{stderr}");
    assert!(stderr.contains("run `piton migrate` to update it"), "{stderr}");
    // Piped, nothing is asked and nothing changes.
    assert_eq!(fixture.config(), OLD);

    let (_, stderr, code) = fixture.run(&["migrate", "--check"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("the project needs a migration"), "{stderr}");
    assert_eq!(fixture.config(), OLD);
}

#[test]
fn piton_migrate_brings_the_project_up_to_date() {
    let fixture = Fixture::new("migrated", OLD);
    let (stdout, stderr, code) = fixture.run(&["migrate"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "piton.config.pi\n");
    assert!(stderr.contains("updated 1 file"), "{stderr}");
    assert_eq!(fixture.config(), OLD.replace("    frameworks:\n", "    plugins:\n"));

    // Up to date: nothing is reported, and the project checks.
    let (_, stderr, code) = fixture.run(&["check"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stderr.contains("frameworks"), "{stderr}");
    let (_, stderr, code) = fixture.run(&["migrate"]);
    assert_eq!(code, 0);
    assert!(stderr.contains("the project is up to date"), "{stderr}");
}

#[test]
fn frameworks_move_to_the_end_of_existing_plugins() {
    let both = OLD.replace(
        "    frameworks:\n        - {Belay}\n",
        "    plugins:\n        - {Tests}\n\n    frameworks:\n        - {Belay}\n",
    ) + "\nuse @piton/test\n\ntest-config Tests:\n    agent: codex\n";
    let fixture = Fixture::new("merged", &both);
    let (_, stderr, code) = fixture.run(&["migrate"]);
    assert_eq!(code, 0, "{stderr}");
    let config = fixture.config();
    assert!(config.contains("    plugins:\n        - {Tests}\n        - {Belay}\n"), "{config}");
    assert!(!config.contains("frameworks"), "{config}");
}

#[test]
fn inline_lists_move_too() {
    let both = OLD.replace(
        "    frameworks:\n        - {Belay}\n",
        "    plugins: [{Tests}]\n    frameworks: [{Belay}]\n",
    ) + "\nuse @piton/test\n\ntest-config Tests:\n    agent: codex\n";
    let fixture = Fixture::new("inline", &both);
    let (_, stderr, code) = fixture.run(&["migrate"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stderr.contains("by hand"), "{stderr}");
    let config = fixture.config();
    assert!(config.contains("    plugins:\n        - {Tests}\n        - {Belay}\n"), "{config}");
    assert!(!config.contains("frameworks"), "{config}");
    let (_, stderr, code) = fixture.run(&["check"]);
    assert_eq!(code, 0, "{stderr}");
}
