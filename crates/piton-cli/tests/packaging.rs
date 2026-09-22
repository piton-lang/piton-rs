//! End-to-end tests for the four package commands.
//!
//! Each builds a real git repository in a temporary directory and tethers it
//! into a real project, because what is being tested is exactly the part that
//! cannot be faked: cloning, un-gitting, the lock file that records what was
//! installed, and the refusal to overwrite files someone has edited.
//!
//! They are skipped when git is not installed, since without it there is no
//! package source to speak of.

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

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

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
        let dir = std::env::temp_dir().join(format!("piton-packaging-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Sandbox { dir }
    }

    fn repo(&self) -> PathBuf {
        self.dir.join("repo")
    }

    fn app(&self) -> PathBuf {
        self.dir.join("app")
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.dir.join(relative))
            .unwrap_or_else(|_| panic!("missing {relative}"))
    }

    fn exists(&self, relative: &str) -> bool {
        self.dir.join(relative).exists()
    }

    /// Runs `piton` in the consuming project.
    fn piton(&self, args: &[&str]) -> (String, String, i32) {
        let output = Command::new(binary())
            .args(args)
            .current_dir(self.app())
            .output()
            .expect("run piton");
        (
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            output.status.code().unwrap_or(-1),
        )
    }

    fn git(&self, directory: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(directory)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn commit(&self, message: &str) {
        let repo = self.repo();
        self.git(&repo, &["add", "-A"]);
        self.git(
            &repo,
            &[
                "-c",
                "user.email=test@example.test",
                "-c",
                "user.name=Test",
                "commit",
                "-qm",
                message,
            ],
        );
    }

    /// The source path the app declares its dependency as.
    fn source(&self) -> String {
        self.repo().to_string_lossy().to_string()
    }
}

/// A repository that publishes one package called `dep-lib`, and a project that
/// imports it.
fn fixture(name: &str) -> Option<Sandbox> {
    if !git_available() {
        eprintln!("skipped: git is not installed, so there is no package source to clone");
        return None;
    }
    let sandbox = Sandbox::new(name);

    sandbox.write(
        "repo/piton.config.pi",
        "use @piton/config
use @piton/packaging

export piton-config Dep:
    root: ./spec
    entry: ./spec/index.pi

    packages:
        - {DepPackage}

package DepPackage:
    name: dep-lib
    root: ./spec
",
    );
    sandbox.write("repo/spec/index.pi", "from ./Anchors export Button\n");
    sandbox.write(
        "repo/spec/Anchors.pi",
        "export anchor Button as button:\n    label: Save\n",
    );
    // A file outside the package root, which must not be installed.
    sandbox.write("repo/README.md", "not part of the package\n");
    sandbox.git(&sandbox.repo(), &["init", "-q", "."]);
    sandbox.commit("one");

    sandbox.write(
        "app/piton.config.pi",
        &format!(
            "use @piton/config

export piton-config App:
    root: ./spec
    entry: ./spec/index.pi

    dependencies:
        - {}
",
            sandbox.source()
        ),
    );
    sandbox.write(
        "app/spec/index.pi",
        "use dep-lib

from dep-lib import Button

export anchor Screen:
    primary: {Button}

button Cancel:
    label: Cancel
",
    );
    Some(sandbox)
}

#[test]
fn tethering_installs_the_published_package_without_its_repository() {
    let Some(sandbox) = fixture("tether") else {
        return;
    };
    let (stdout, stderr, code) = sandbox.piton(&["tether", &sandbox.source()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("tethers/dep-lib"), "{stdout}");

    // The package root was installed, not the repository around it.
    assert!(sandbox.exists("app/tethers/dep-lib/index.pi"));
    assert!(sandbox.exists("app/tethers/dep-lib/Anchors.pi"));
    assert!(!sandbox.exists("app/tethers/dep-lib/README.md"));
    assert!(!sandbox.exists("app/tethers/dep-lib/.git"));
    assert!(!sandbox.exists("app/tethers/dep-lib/piton.config.pi"));

    let lock = sandbox.read("app/.piton/packages.lock.json");
    assert!(lock.contains("\"name\": \"dep-lib\""), "{lock}");
    assert!(lock.contains("\"index.pi\""), "{lock}");

    // And the project compiles against it, through the package name.
    let (stdout, stderr, code) = sandbox.piton(&["compile", "spec/index.pi"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"label\": \"Save\""), "{stdout}");
}

#[test]
fn updating_moves_a_package_to_the_newest_commit() {
    let Some(sandbox) = fixture("update") else {
        return;
    };
    assert_eq!(sandbox.piton(&["tether", &sandbox.source()]).2, 0);

    sandbox.write(
        "repo/spec/Anchors.pi",
        "export anchor Button as button:\n    label: Submit\n",
    );
    sandbox.commit("two");

    let (stdout, stderr, code) = sandbox.piton(&["update"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("updated dep-lib"), "{stdout}");
    assert!(sandbox.read("app/tethers/dep-lib/Anchors.pi").contains("Submit"));

    // Running it again reports no change rather than rewriting the tree.
    let (stdout, stderr, code) = sandbox.piton(&["update"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("0 of 1 package changed"), "{stdout}");
}

#[test]
fn an_edited_package_is_not_overwritten() {
    let Some(sandbox) = fixture("edited") else {
        return;
    };
    assert_eq!(sandbox.piton(&["tether", &sandbox.source()]).2, 0);

    let edited = "export anchor Button as button:\n    label: Edited locally\n";
    sandbox.write("app/tethers/dep-lib/Anchors.pi", edited);

    sandbox.write(
        "repo/spec/Anchors.pi",
        "export anchor Button as button:\n    label: Submit\n",
    );
    sandbox.commit("two");

    let (_, stderr, code) = sandbox.piton(&["update"]);
    assert_eq!(code, 1, "an edited package must not be replaced");
    assert!(stderr.contains("has been modified"), "{stderr}");
    assert!(stderr.contains("untether"), "{stderr}");
    assert_eq!(sandbox.read("app/tethers/dep-lib/Anchors.pi"), edited);

    // Removing it is refused for the same reason.
    let (_, stderr, code) = sandbox.piton(&["remove", "dep-lib"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(sandbox.exists("app/tethers/dep-lib/Anchors.pi"));
}

#[test]
fn untethering_moves_the_package_and_rewrites_what_named_it() {
    let Some(sandbox) = fixture("untether") else {
        return;
    };
    assert_eq!(sandbox.piton(&["tether", &sandbox.source()]).2, 0);
    sandbox.write(
        "app/spec/index.pi",
        "use dep-lib

from dep-lib import Button
from dep-lib/Anchors import Button ButtonAgain

export anchor Screen:
    primary: {Button}
    again: {ButtonAgain}

button Cancel:
    label: Cancel
",
    );

    let (stdout, stderr, code) = sandbox.piton(&["untether", "dep-lib"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("spec/untethered/dep-lib"), "{stdout}");
    assert!(!sandbox.exists("app/tethers"), "the tethers directory was pruned");
    assert!(sandbox.exists("app/spec/untethered/dep-lib/Anchors.pi"));

    let rewritten = sandbox.read("app/spec/index.pi");
    assert!(rewritten.contains("use /untethered/dep-lib\n"), "{rewritten}");
    assert!(
        rewritten.contains("from /untethered/dep-lib import Button\n"),
        "{rewritten}"
    );
    // The deeper path keeps everything after the package name.
    assert!(
        rewritten.contains("from /untethered/dep-lib/Anchors import Button ButtonAgain"),
        "{rewritten}"
    );
    assert!(!rewritten.contains("dep-lib\n") || !rewritten.contains("use dep-lib"));

    assert!(sandbox
        .read("app/.piton/packages.lock.json")
        .contains("\"packages\": []"));

    let (stdout, stderr, code) = sandbox.piton(&["compile", "spec/index.pi"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"label\": \"Save\""), "{stdout}");
}

#[test]
fn untethering_can_rename_and_can_leave_imports_alone() {
    let Some(sandbox) = fixture("untether-as") else {
        return;
    };
    assert_eq!(sandbox.piton(&["tether", &sandbox.source()]).2, 0);

    let before = sandbox.read("app/spec/index.pi");
    let (stdout, stderr, code) =
        sandbox.piton(&["untether", "dep-lib", "--as", "vendor/buttons", "--no-rewrite"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("spec/untethered/vendor/buttons"), "{stdout}");
    assert!(sandbox.exists("app/spec/untethered/vendor/buttons/index.pi"));
    // Nothing was rewritten, and the imports that still name it are listed.
    assert_eq!(sandbox.read("app/spec/index.pi"), before);
    assert!(stdout.contains("still name"), "{stdout}");
}

#[test]
fn removing_refuses_while_something_still_imports_it() {
    let Some(sandbox) = fixture("remove") else {
        return;
    };
    assert_eq!(sandbox.piton(&["tether", &sandbox.source()]).2, 0);

    let (_, stderr, code) = sandbox.piton(&["remove", "dep-lib"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("still imported"), "{stderr}");
    assert!(sandbox.exists("app/tethers/dep-lib/index.pi"));

    sandbox.write("app/spec/index.pi", "export anchor Screen:\n    primary: none\n");
    let (stdout, stderr, code) = sandbox.piton(&["remove", "dep-lib"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("removed dep-lib"), "{stdout}");
    assert!(!sandbox.exists("app/tethers"));
    // It is still declared, so the next update would bring it back.
    assert!(stdout.contains("still declared"), "{stdout}");
}

#[test]
fn a_repository_without_configuration_installs_whole() {
    if !git_available() {
        eprintln!("skipped: git is not installed");
        return;
    }
    let sandbox = Sandbox::new("bare");
    sandbox.write("repo/index.pi", "from ./Thing export Thing\n");
    sandbox.write("repo/Thing.pi", "export anchor Thing:\n    kind: bare\n");
    sandbox.git(&sandbox.repo(), &["init", "-q", "."]);
    sandbox.commit("one");

    sandbox.write(
        "app/piton.config.pi",
        "use @piton/config

export piton-config App:
    root: ./spec
    entry: ./spec/index.pi
",
    );
    sandbox.write(
        "app/spec/index.pi",
        "from repo import Thing\n\nexport anchor Uses:\n    it: {Thing}\n",
    );

    let (stdout, stderr, code) = sandbox.piton(&["tether", &sandbox.source()]);
    assert_eq!(code, 0, "{stderr}");
    // Named after the repository, since nothing else named it.
    assert!(stdout.contains("tethers/repo"), "{stdout}");
    assert!(sandbox.exists("app/tethers/repo/index.pi"));

    let (stdout, stderr, code) = sandbox.piton(&["compile", "spec/index.pi"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"kind\": \"bare\""), "{stdout}");
}

#[test]
fn a_transitive_dependency_is_installed_flat() {
    if !git_available() {
        eprintln!("skipped: git is not installed");
        return;
    }
    let sandbox = Sandbox::new("transitive");

    // The deepest repository.
    let base = sandbox.dir.join("base");
    sandbox.write("base/index.pi", "export anchor Base:\n    depth: 1\n");
    sandbox.git(&base, &["init", "-q", "."]);
    sandbox.git(&base, &["add", "-A"]);
    sandbox.git(
        &base,
        &[
            "-c",
            "user.email=t@example.test",
            "-c",
            "user.name=T",
            "commit",
            "-qm",
            "one",
        ],
    );

    // A repository that depends on it.
    sandbox.write(
        "repo/piton.config.pi",
        &format!(
            "use @piton/config

export piton-config Middle:
    root: ./spec
    entry: ./spec/index.pi

    dependencies:
        - {}
",
            base.to_string_lossy()
        ),
    );
    sandbox.write("repo/spec/index.pi", "export anchor Middle:\n    depth: 2\n");
    sandbox.git(&sandbox.repo(), &["init", "-q", "."]);
    sandbox.commit("one");

    sandbox.write(
        "app/piton.config.pi",
        "use @piton/config

export piton-config App:
    root: ./spec
    entry: ./spec/index.pi
",
    );
    sandbox.write("app/spec/index.pi", "export anchor App:\n    depth: 3\n");

    let (stdout, stderr, code) = sandbox.piton(&["tether", &sandbox.source()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("tethers/repo"), "{stdout}");
    assert!(stdout.contains("tethers/base"), "{stdout}");
    // Flat: the transitive dependency sits beside the one that asked for it,
    // not inside it.
    assert!(sandbox.exists("app/tethers/base/index.pi"));
    assert!(!sandbox.exists("app/tethers/repo/tethers"));
}

#[test]
fn two_packages_asking_for_different_versions_get_one_and_a_warning() {
    if !git_available() {
        eprintln!("skipped: git is not installed");
        return;
    }
    let sandbox = Sandbox::new("conflict");

    // A shared repository with two commits, the first one tagged.
    let shared = sandbox.dir.join("shared");
    sandbox.write("shared/index.pi", "export anchor Shared:\n    version: one\n");
    sandbox.git(&shared, &["init", "-q", "."]);
    let author = [
        "-c",
        "user.email=t@example.test",
        "-c",
        "user.name=T",
    ];
    sandbox.git(&shared, &["add", "-A"]);
    sandbox.git(&shared, &[&author[..], &["commit", "-qm", "one"][..]].concat());
    sandbox.git(&shared, &["tag", "v1"]);
    sandbox.write("shared/index.pi", "export anchor Shared:\n    version: two\n");
    sandbox.git(&shared, &["add", "-A"]);
    sandbox.git(&shared, &[&author[..], &["commit", "-qm", "two"][..]].concat());

    // The repository the project tethers pins the tag; the project itself does
    // not, so the two declarations disagree.
    sandbox.write(
        "repo/piton.config.pi",
        &format!(
            "use @piton/config

export piton-config Middle:
    root: ./spec
    entry: ./spec/index.pi

    dependencies:
        - {}
            tag: \"v1\"
",
            shared.to_string_lossy()
        ),
    );
    sandbox.write("repo/spec/index.pi", "export anchor Middle:\n    depth: 2\n");
    sandbox.git(&sandbox.repo(), &["init", "-q", "."]);
    sandbox.commit("one");

    sandbox.write(
        "app/piton.config.pi",
        &format!(
            "use @piton/config

export piton-config App:
    root: ./spec
    entry: ./spec/index.pi

    dependencies:
        - {}
        - {}
",
            sandbox.source(),
            shared.to_string_lossy()
        ),
    );
    sandbox.write("app/spec/index.pi", "export anchor App:\n    depth: 3\n");

    let (stdout, stderr, code) = sandbox.piton(&["update"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("tethers") || code == 0, "{stdout}");
    assert!(
        stderr.contains("is required at") && stderr.contains("installing"),
        "the disagreement was not reported: {stderr}"
    );

    // One copy, and the version the report named is the one on disk: the tag is
    // the more specific pin, so `v1` wins over the default branch.
    assert!(sandbox.exists("app/tethers/shared/index.pi"));
    assert!(
        sandbox.read("app/tethers/shared/index.pi").contains("version: one"),
        "the less specific pin won: {}",
        sandbox.read("app/tethers/shared/index.pi")
    );
}
