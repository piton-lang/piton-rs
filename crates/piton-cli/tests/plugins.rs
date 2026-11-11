//! Plugins installed in a project, end to end.
//!
//! The bundled test plugin tests the work of a real plugin. These tests
//! examine the edge cases for a plugin author. They test a plugin declared in
//! the project, its settings, and the compile queries. They also test that
//! the host refuses each capability that the plugin did not ask for.

#![cfg(unix)]

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
        let dir = std::env::temp_dir().join(format!("piton-plugins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Fixture { dir }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
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

const CONFIG: &str = "use @piton/config
use @piton/plugin

export piton-config App:
    root: ./spec
    plugins:
        - {Hello}
        - {Greeting}

export abstract anchor HelloSettings as hello-settings:
    greeting:: string:: null: null

hello-settings Greeting:
    greeting: Howdy

piton-plugin Hello:
    name: hello
    entry: ./plugin/index.ts
    settings: HelloSettings
    description: Says hello, and tries what it may not do.

    capabilities:
        process: [echo]

    commands:
        hello:
            about: Say hello
            arguments:
                who:
                    help: Who to greet
                    positional: true
                times:
                    type: number
                    minimum: 1
                loud:
                    type: flag
                probe:
                    type: flag
";

const PLUGIN: &str = r#"import { definePlugin, type Context, type Json } from "@piton/plugin";
import { shout } from "./shout";

interface Args {
  who: string | null;
  times: number | null;
  loud: boolean;
  probe: boolean;
}

async function hello(ctx: Context, raw: Record<string, Json>): Promise<number> {
  const args = raw as unknown as Args;
  const greeting = (ctx.settings()?.properties.greeting as string | null) ?? "Hello";
  let line = `${greeting}, ${args.who ?? "world"}`;
  if (args.loud) line = shout(line);
  for (let i = 0; i < (args.times ?? 1); i += 1) ctx.stdout(`${line}\n`);

  const echoed = ctx.spawn("echo", ["from", "echo"]);
  for await (const { stream, line } of echoed) ctx.stdout(`${stream}: ${line}\n`);
  ctx.stdout(`echo exited ${await echoed.wait()}\n`);

  const compiled = ctx.compile({ workspace: true });
  const names = compiled.anchors({ exported: true, withinSourceRoot: true }).map((anchor) => anchor.name);
  ctx.stdout(`anchors: ${names.join(", ")}\n`);
  const thing = compiled.anchors({ exported: true, withinSourceRoot: true })[0];
  ctx.stdout(`colour: ${thing.properties.colour}\n`);

  if (args.probe) {
    for (const attempt of [
      () => ctx.spawn("ls", []),
      () => ctx.readFile("piton.config.pi"),
      () => ctx.temporaryFile("x"),
      () => ctx.onPath("ls"),
      () => ctx.choose("Pick one", ["a", "b"]),
    ]) {
      try {
        attempt();
        ctx.stdout("allowed\n");
      } catch (error) {
        ctx.stdout(`refused: ${(error as Error).message}\n`);
      }
    }
  }
  ctx.report.done("said hello");
  return args.loud ? 3 : 0;
}

export default definePlugin({ commands: { hello } });
"#;

fn project(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fixture.write("piton.config.pi", CONFIG);
    fixture.write("plugin/index.ts", PLUGIN);
    fixture.write(
        "plugin/shout.ts",
        "export function shout(text: string): string {\n  return text.toUpperCase() + \"!\";\n}\n",
    );
    fixture.write("spec/index.pi", "export anchor Thing:\n    colour: blue\n");
    fixture
}

#[test]
fn an_installed_plugin_adds_its_command() {
    let fixture = project("command");
    let (stdout, _, code) = fixture.run(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("hello") && stdout.contains("Say hello"), "{stdout}");

    let (stdout, stderr, code) = fixture.run(&["hello", "Ada", "--times", "2"]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    // Its settings, its own module, a process that it can run, and the
    // compiled project.
    assert_eq!(stdout.matches("Howdy, Ada\n").count(), 2, "{stdout}");
    assert!(stdout.contains("stdout: from echo\necho exited 0\n"), "{stdout}");
    assert!(stdout.contains("anchors: Thing\n"), "{stdout}");
    assert!(stdout.contains("colour: blue\n"), "{stdout}");
    assert!(stderr.contains("✓ said hello"), "{stderr}");

    // The return number is the exit code.
    let (stdout, _, code) = fixture.run(&["hello", "Ada", "--loud"]);
    assert_eq!(code, 3);
    assert!(stdout.contains("HOWDY, ADA!"), "{stdout}");

    // The CLI checks the arguments with the declaration, before the command
    // runs.
    let (_, stderr, code) = fixture.run(&["hello", "--times", "0"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("it has to be at least 1"), "{stderr}");
}

#[test]
fn what_a_plugin_did_not_declare_is_refused() {
    let fixture = project("capabilities");
    let (stdout, stderr, code) = fixture.run(&["hello", "--probe"]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let refused: Vec<&str> = stdout.lines().filter(|line| line.starts_with("refused: ")).collect();
    assert_eq!(refused.len(), 5, "{stdout}");
    assert!(refused[0].contains("may not run `ls`") && refused[0].contains("capabilities.process"), "{stdout}");
    assert!(refused[1].contains("may not read files"), "{stdout}");
    assert!(refused[2].contains("may not write temporary files"), "{stdout}");
    assert!(refused[3].contains("may not look for `ls`"), "{stdout}");
    assert!(refused[4].contains("may not ask the user to choose") && refused[4].contains("capabilities.interactive"), "{stdout}");
}

#[test]
fn piton_plugins_lists_what_each_may_do() {
    let fixture = project("listing");
    let (stdout, stderr, code) = fixture.run(&["plugins"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("test (bundled)"), "{stdout}");
    assert!(stdout.contains("hello (piton.config.pi)"), "{stdout}");
    assert!(stdout.contains("may: run echo"), "{stdout}");
    assert!(stdout.contains("settings: HelloSettings"), "{stdout}");
    assert!(stderr.contains("5 plugins"), "{stderr}");
    assert!(stdout.contains("init (bundled)") && stdout.contains("start new projects"), "{stdout}");
    assert!(stdout.contains("migrate (bundled)") && stdout.contains("propose edits to piton.config.pi"), "{stdout}");
    assert!(stdout.contains("belay (bundled)"), "{stdout}");
}

#[test]
fn a_plugin_that_fails_is_reported() {
    let fixture = project("failing");
    fixture.write(
        "plugin/index.ts",
        "import { definePlugin } from \"@piton/plugin\";\nexport default definePlugin({ commands: { hello() { throw new Error(\"no greeting today\"); } } });\n",
    );
    let (_, stderr, code) = fixture.run(&["hello"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("plugin `hello` failed: no greeting today"), "{stderr}");

    // Its TypeScript does not parse.
    fixture.write("plugin/index.ts", "export default {\n  commands: { hello( { }\n");
    let (_, stderr, code) = fixture.run(&["hello"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("plugin `hello` failed") && stderr.contains("plugin/index.ts:"), "{stderr}");

    // It imports a module that is not its own.
    fixture.write("plugin/index.ts", "import fs from \"node:fs\";\nexport default { commands: { hello: () => fs.readFileSync(\"x\") } };\n");
    let (_, stderr, code) = fixture.run(&["hello"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("can import @piton/plugin and its own files"), "{stderr}");
}

#[test]
fn an_unknown_entry_under_plugins_is_warned_about() {
    let fixture = project("unclaimed");
    fixture.write(
        "piton.config.pi",
        &(CONFIG.replace("        - {Greeting}\n", "        - {Greeting}\n        - {Stray}\n")
            + "\nanchor Stray:\n    pass\n"),
    );
    let (_, stderr, _) = fixture.run(&["plugins"]);
    assert!(stderr.contains("`Stray` is not a plugin, or the settings of one"), "{stderr}");
}

#[test]
fn a_command_piton_has_is_not_replaced() {
    let fixture = project("builtin");
    fixture.write(
        "piton.config.pi",
        &CONFIG.replace("        hello:\n            about: Say hello\n", "        check:\n            about: Not the real check\n        hello:\n            about: Say hello\n"),
    );
    let (_, stderr, code) = fixture.run(&["check"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("no problems found"), "{stderr}");
    let (_, stderr, _) = fixture.run(&["plugins"]);
    assert!(stderr.contains("adds the command `check`, which piton already has"), "{stderr}");
}

/// A stand-in for an agentic tool. It records its arguments and its OpenCode
/// configuration. Then it exits with 7.
const FAKE_TOOL: &str = "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/../$(basename \"$0\").args\"\nprintf '%s' \"$OPENCODE_CONFIG_CONTENT\" > \"$(dirname \"$0\")/../$(basename \"$0\").env\"\nexit 7\n";

#[test]
fn piton_agent_launches_each_tool_with_fluency_through_belay() {
    let fixture = project("agent");
    for tool in ["claude", "codex", "opencode", "muse"] {
        fixture.write(&format!("bin/{tool}"), FAKE_TOOL);
        std::fs::set_permissions(
            fixture.dir.join(format!("bin/{tool}")),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .expect("chmod");
    }
    let run = |args: &[&str]| -> (i32, Vec<String>, String) {
        let path = format!("{}:/usr/bin:/bin", fixture.dir.join("bin").display());
        let output = Command::new(binary())
            .args(args)
            .current_dir(&fixture.dir)
            .env("PATH", path)
            .output()
            .expect("run piton");
        let tool = args.get(1).copied().unwrap_or("claude");
        let recorded = std::fs::read_to_string(fixture.dir.join(format!("{tool}.args"))).unwrap_or_default();
        let env = std::fs::read_to_string(fixture.dir.join(format!("{tool}.env"))).unwrap_or_default();
        (
            output.status.code().unwrap_or(-1),
            recorded.lines().map(str::to_string).collect(),
            env,
        )
    };

    // The exit code of the tool is the exit code of the command. The
    // arguments, with the options, go to the tool unchanged.
    let (code, args, _) = run(&["agent", "claude", "--resume"]);
    assert_eq!(code, 7);
    // Claude reads the full prompt from a temporary file.
    assert_eq!(args[0], "--append-system-prompt-file");
    assert!(args[1].contains("piton-belay-"), "{args:?}");
    assert_eq!(args.last().map(String::as_str), Some("--resume"));

    // The CLI gives a short prompt to Codex. The system limits the size of one
    // argument, and the full prompt is larger. Without a built skill, the
    // short prompt points to a temporary file that holds the reference.
    let (_, args, _) = run(&["agent", "codex"]);
    assert_eq!(args[0], "-c");
    assert!(args[1].starts_with("developer_instructions=\""), "{:?}", &args[..2]);
    let text = args.join("\n");
    assert!(text.contains("This project is written in Piton"), "{text}");
    assert!(text.contains("piton-belay-") && text.len() < 4096, "{text}");

    // With a built skill, the short prompt points to the skill.
    fixture.write(".agents/skills/piton-fluency/SKILL.md", "---\nname: piton-fluency\n---\nFluency.\n");
    let (_, args, _) = run(&["agent", "codex"]);
    let text = args.join("\n");
    assert!(text.contains("`piton-fluency` skill, at `.agents/skills/piton-fluency/SKILL.md`"), "{text}");

    // Muse takes the prompt after `exec`, and runs `resume` without it.
    let (_, args, _) = run(&["agent", "muse", "exec", "hi"]);
    assert_eq!(&args[..2], ["exec", "-c"]);
    assert!(args.join("\n").contains(".agents/skills/piton-fluency/SKILL.md"), "{args:?}");
    let (_, args, _) = run(&["agent", "muse", "resume", "--last"]);
    assert_eq!(args, ["resume", "--last"]);

    // OpenCode reads the prompt from a file named in its configuration.
    let (_, args, env) = run(&["agent", "opencode"]);
    assert_eq!(args, ["--standalone"]);
    assert!(env.contains("\"instructions\":[") && env.contains("piton-belay-"), "{env}");

    // The output names a tool that is not installed, and tells the
    // alternative.
    std::fs::remove_file(fixture.dir.join("bin/codex")).expect("remove");
    let output = Command::new(binary())
        .args(["agent", "codex"])
        .current_dir(&fixture.dir)
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.dir.join("bin").display()))
        .env("NO_COLOR", "1")
        .output()
        .expect("run piton");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`codex` is not on PATH; install it or print the prompt with `piton agent codex --print-fluency`"),
        "{stderr}"
    );
}
