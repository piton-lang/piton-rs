// This module gives one test to a tool for a verdict.
//
// For each test, the tool starts one session in headless mode.
// The tool receives a request. The request gives the test.
// The request also asks for a verdict line at the end.
// This module reads the output of the tool while the tool writes it.
// Thus, the run can show what the tool does.
// The verdict comes from the answer of the tool.

import type { Context, Project as HostProject } from "@piton/plugin";

/** The project, with the code root that Belay configures, or null when the
 * project does not configure Belay. */
export interface Project extends HostProject {
  codeRoot: string | null;
}

/** This marker starts the verdict line of a tool. */
export const VERDICT = "PITON-VERDICT:";

/** This text tells the tool how to explain a failure. ASD-STE100 is
 * Simplified Technical English. It is a controlled language. Readers can
 * understand it quickly, also when English is not their first language. This
 * text gives the most important rules of ASD-STE100. */
const STE =
  "When pass is false, also give an explanation of why the statement is not true, " +
  "written in ASD-STE100 Simplified Technical English. Keep it short: at most four sentences, on " +
  "the one line. Use only words from the ASD-STE100 dictionary with their approved meanings, plus " +
  "technical names such as file, anchor and property names. Use one word for one meaning. Write " +
  "sentences of no more than 20 words, with one topic each. Use the active voice and the simple " +
  "present, past or future tense. Do not use -ing forms of verbs, contractions or phrasal verbs. " +
  'Use articles such as "the" and "a". Keep noun clusters to three words or fewer. Tell what is ' +
  "missing or different, and where.";

/** What a fix may change, for each scope. The fix and the fix prompt use the
 * same words, so the tool and the agent that fixes agree on the target. */
function target(project: Project, against: string): { what: string; only: string; keep: string } {
  const root = project.root;
  const spec = `the specification (the \`.pi\` files under \`${relative(project.sourceRoot, root)}\`)`;
  const code =
    project.codeRoot === null
      ? `the code (the files of the project outside \`${relative(project.sourceRoot, root)}\`, \`tethers/\` and the generated agent directories)`
      : `the code (the files under \`${relative(project.codeRoot, root)}\`)`;
  if (against === "spec") return { what: spec, only: "the specification", keep: "Do not change the code." };
  if (against === "code") return { what: code, only: "the code", keep: "Do not change the specification." };
  return { what: `${spec} and ${code}`, only: "the specification and the code", keep: "" };
}

/** This text tells the tool how to write the fix for a failure. The fix goes
 * into a prompt for an agent that will change the project. Thus, it is also
 * in ASD-STE100. The fix changes only what the test examines. */
function fixRequest(project: Project, against: string): string {
  const { what, keep } = target(project, against);
  const both =
    against === "both"
      ? " Give the changes to the specification and the changes to the code as two parts. " +
        "Give a part only when that side does not make the statement true. " +
        "After the fix, the specification and the code must agree."
      : ` ${keep}`;
  return (
    "When pass is false, also give a fix: the instructions an agent needs to make the statement " +
    `true in ${what}. Write them in ASD-STE100 Simplified Technical English, as commands, on the ` +
    "one line. Name each file to change and tell what to change in it. Change the specification " +
    `in its .pi source, not in generated files.${both}`
  );
}

/** An agent writes these directories for itself. They are not part of the
 * code of the project. */
const GENERATED = [".claude", ".codex", ".agents", ".opencode", ".muse", ".piton"];

/** One test that is ready to run. */
export interface TestCase {
  name: string;
  /** An absolute path. */
  file: string;
  expects: string;
  description: string | null;
  against: string;
  tags: string[];
}

/** The result on one side of a test against both: the specification or the
 * code. */
export interface Side {
  pass: boolean;
  reason: string;
}

/** The two results of a test against both. Null for a test against one side,
 * or when the tool did not give the two results. */
export type Sides = { spec: Side; code: Side } | null;

export type Outcome =
  | { status: "pass"; reason: string; sides: Sides }
  | { status: "fail"; reason: string; explanation: string | null; fix: string | null; sides: Sides }
  /** The tool failed, or did not give a verdict. Thus, the test has no verdict. */
  | { status: "error"; reason: string };

/** Gives a path relative to the root of the project. The tool starts in the
 * root of the project. */
export function relative(path: string, root: string): string {
  if (path === root) return ".";
  if (path.startsWith(root + "/")) return path.slice(root.length + 1);
  return path;
}

/** Makes the request for one test. */
export function request(project: Project, test: TestCase): string {
  const root = project.root;
  const spec = relative(project.sourceRoot, root);
  const code = relative(project.codeRoot ?? root, root);
  const output = relative(project.outputDir, root);
  const excluded =
    `leaving out the specification under \`${spec}\`, \`tethers/\`, the build output under \`${output}\`, ` +
    `and the generated agent directories (${GENERATED.map((name) => `\`${name}\``).join(", ")})`;
  let scope: string;
  if (test.against === "spec") {
    scope = `the specification only: the \`.pi\` files under \`${spec}\`. Do not use the implementation as evidence.`;
  } else if (test.against === "code") {
    scope = `the code only: the files under \`${code}\`, ${excluded}. Do not use the specification as evidence.`;
  } else {
    scope =
      `both the specification (the \`.pi\` files under \`${spec}\`) and the code (the files under \`${code}\`, ${excluded}). ` +
      "Judge the specification and the code separately. The test passes only if the statement is true in each.";
  }
  const description = test.description === null ? "" : `\nContext from the test's author:\n\n${test.description}\n`;
  return (
    "You are judging one test of this Piton project. Decide whether this statement is true:\n" +
    "\n" +
    `${test.expects}\n` +
    `${description}\n` +
    `The test is \`${test.name}\`, declared in \`${relative(test.file, root)}\`. Judge it against ${scope}\n` +
    "\n" +
    "Read whatever you need, but do not change, create or delete any file. Base the verdict " +
    "on what you find, not on what is likely: the statement fails when the evidence " +
    "contradicts it or is not there.\n" +
    "\n" +
    "End your answer with exactly one line in this form, with pass true or false:\n" +
    "\n" +
    `${VERDICT} {"pass": <true or false>, "reason": "<one sentence>", ${
      test.against === "both"
        ? '"spec": {"pass": <true or false>, "reason": "<one sentence about the specification>"}, ' +
          '"code": {"pass": <true or false>, "reason": "<one sentence about the code>"}, '
        : ""
    }"explanation": "<only when pass is false>", "fix": "<only when pass is false>"}\n` +
    "\n" +
    `${STE}\n` +
    "\n" +
    fixRequest(project, test.against)
  );
}

/** The data that all sessions of the tool use. */
export interface Launcher {
  agent: string;
  program: string;
  /** The options that select the model, empty for the tool's default. */
  modelOptions: string[];
  /** The prompt as text. Codex and Muse take it as an argument, so it is
   * the short prompt that points to the reference. */
  fluency: string;
  /** The file that holds the full prompt. Claude and OpenCode read it. */
  promptFile: string | null;
}

/** Gives the arguments and the environment variables for one request. The
 * tool operates in headless mode. Where the tool can prevent changes, it
 * operates in read-only mode. */
export function launch(launcher: Launcher, request: string): { args: string[]; env: Record<string, string> } {
  switch (launcher.agent) {
    case "claude":
      return {
        args: [
          "-p",
          request,
          "--append-system-prompt-file",
          launcher.promptFile ?? "",
          "--allowedTools",
          "Read,Grep,Glob",
          "--disallowedTools",
          "Edit,Write,NotebookEdit,Bash",
          // The tool sends events when they occur.
          // Thus, the run can show what the tool does.
          // The verdict comes from the messages of the tool.
          "--output-format",
          "stream-json",
          "--verbose",
          ...launcher.modelOptions,
        ],
        env: {},
      };
    case "codex":
      return {
        args: [
          "exec",
          "--sandbox",
          "read-only",
          "--skip-git-repo-check",
          "-c",
          // A JSON string is also a valid TOML basic string.
          `developer_instructions=${JSON.stringify(launcher.fluency)}`,
          ...launcher.modelOptions,
          request,
        ],
        env: {},
      };
    case "opencode":
      return {
        args: ["run", "--standalone", ...launcher.modelOptions, request],
        env: {
          OPENCODE_CONFIG_CONTENT: JSON.stringify({
            instructions: [launcher.promptFile],
            permission: { edit: "deny", bash: "deny" },
          }),
        },
      };
    case "muse":
      return {
        args: ["exec", "-c", `run.developer_prompt=${JSON.stringify(launcher.fluency)}`, ...launcher.modelOptions, request],
        env: {},
      };
    default:
      throw new Error(`no launcher for \`${launcher.agent}\``);
  }
}

/** The result of one line of output from a tool. */
export interface Read {
  /** The activity of the tool, if the line shows one. */
  activity: string | null;
  /** The text that this line adds to the transcript. The verdict is in the
   * transcript. */
  text: string;
}

/** Reads one line of output. A Claude event comes from `--output-format
 * stream-json`. Its tool calls give the activity. Its messages go into the
 * transcript. Another line gives the activity and the transcript. */
export function readLine(line: string, root: string): Read {
  const plain: Read = { activity: line.trim() || null, text: `${line}\n` };
  let event: any;
  try {
    event = JSON.parse(line);
  } catch {
    return plain;
  }
  if (event === null || typeof event !== "object" || typeof event.type !== "string") return plain;
  const read: Read = { activity: null, text: "" };
  if (event.type === "system" && event.subtype === "init") {
    // The session started. The model now does its first turn.
    // This can be the longest wait of the run.
    read.activity = "thinking";
  } else if (event.type === "assistant") {
    for (const block of event.message?.content ?? []) {
      if (block?.type === "tool_use") {
        read.activity = describeTool(block, root);
      } else if (block?.type === "text") {
        const text = String(block.text ?? "");
        const first = text.split("\n").map((line) => line.trim()).find((line) => line.length > 0);
        if (first !== undefined) read.activity = first;
        read.text += `${text}\n`;
      } else if (block?.type === "thinking") {
        read.activity = "thinking";
      }
    }
  } else if (event.type === "result") {
    if (typeof event.result === "string") read.text += `${event.result}\n`;
    read.activity = "finishing";
  }
  return read;
}

/** Shows a tool call in a few words. Paths are relative to the project. */
function describeTool(block: any, root: string): string {
  const input = (key: string): string | null =>
    typeof block.input?.[key] === "string" ? relative(block.input[key], root) : null;
  switch (block.name) {
    case "Read":
      return `reading ${input("file_path") ?? ""}`;
    case "Grep": {
      const path = input("path");
      const pattern = input("pattern") ?? "";
      return path === null ? `searching for ${pattern}` : `searching ${path} for ${pattern}`;
    }
    case "Glob":
      return `finding ${input("pattern") ?? ""}`;
    default:
      return `using ${block.name ?? "a tool"}`;
  }
}

/** The verdict of a tool on one test. */
export interface Verdict {
  pass: boolean;
  reason: string;
  /** The reason for a failure, in ASD-STE100. Only a failure keeps it. */
  explanation: string | null;
  /** The instructions to fix a failure, in ASD-STE100. Only a failure keeps them. */
  fix: string | null;
  /** The result in the specification and in the code, for a test against both. */
  sides: Sides;
}

/** Finds the verdict in the output of a tool. The verdict is the last line
 * with the marker and a JSON object. The object must have a boolean `pass`.
 * Other lines with the marker do not count. An example is the request that
 * the tool shows again. */
export function verdict(output: string, both = false): Verdict | null {
  const lines = output.split("\n");
  for (let i = lines.length - 1; i >= 0; i -= 1) {
    const at = lines[i].indexOf(VERDICT);
    if (at < 0) continue;
    const json = lines[i].slice(at + VERDICT.length).trim().replace(/^`+|`+$/g, "").trim();
    let value: any;
    try {
      value = JSON.parse(json);
    } catch {
      continue;
    }
    if (value === null || typeof value !== "object" || typeof value.pass !== "boolean") continue;
    const text = (key: string): string | null =>
      typeof value[key] === "string" && value[key].trim().length > 0 ? value[key].trim() : null;
    const side = (key: string): Side | null => {
      const part = value[key];
      if (part === null || typeof part !== "object" || typeof part.pass !== "boolean") return null;
      return { pass: part.pass, reason: typeof part.reason === "string" ? part.reason.trim() : "" };
    };
    const spec = side("spec");
    const code = side("code");
    const sides: Sides = both && spec !== null && code !== null ? { spec, code } : null;
    // With two results, the test passes only when both pass, whatever the
    // overall value says.
    const pass = sides === null ? value.pass : sides.spec.pass && sides.code.pass;
    return {
      pass,
      reason: text("reason") ?? "",
      explanation: pass ? null : text("explanation"),
      fix: pass ? null : text("fix"),
      sides,
    };
  }
  return null;
}

/** Gives the last lines of `text` that are not empty, as one line. An error
 * message uses them. */
function lastLines(text: string, count: number): string | null {
  const lines = text.split("\n").map((line) => line.trim()).filter((line) => line.length > 0);
  return lines.length === 0 ? null : lines.slice(-count).join(" ");
}

/** Runs one test and gives its result. While the test runs, it tells what
 * the tool does. */
export async function judge(
  ctx: Context,
  project: Project,
  launcher: Launcher,
  test: TestCase,
  onActivity: (activity: string) => void,
): Promise<Outcome> {
  const program = launcher.program;
  const { args, env } = launch(launcher, request(project, test));
  let process;
  try {
    process = ctx.spawn(program, args, { env });
  } catch (error) {
    return { status: "error", reason: `cannot launch \`${program}\`: ${(error as Error).message}` };
  }
  let transcript = "";
  let errors = "";
  for await (const { stream, line } of process) {
    if (stream === "stderr") {
      if (line.trim().length > 0) onActivity(line.trim());
      errors += `${line}\n`;
    } else {
      const read = readLine(line, project.root);
      if (read.activity !== null) onActivity(read.activity);
      transcript += read.text;
    }
  }
  const code = await process.wait();
  if (code !== 0) {
    const status = code === null ? `\`${program}\` was stopped by a signal` : `\`${program}\` exited with code ${code}`;
    const detail = lastLines(errors, 3) ?? lastLines(transcript, 3);
    return { status: "error", reason: detail === null ? status : `${status}: ${detail}` };
  }
  const both = test.against === "both";
  const found = verdict(transcript, both) ?? verdict(errors, both);
  if (found === null) return { status: "error", reason: `\`${program}\` ended without a verdict line` };
  const sides = found.sides;
  return found.pass
    ? { status: "pass", reason: found.reason, sides }
    : { status: "fail", reason: found.reason, explanation: found.explanation, fix: found.fix, sides };
}

/** Makes the prompt that fixes one failed test. The prompt is complete: an
 * agent with no other context can use it. The prompt aims at the scope of the
 * test. For spec, it fixes the specification. For code, it fixes the code. For
 * both, it fixes the two, and they must agree. The fix from the tool tells
 * what to change. When the tool gave no fix, the prompt tells the agent to
 * find the differences. */
export function fixPrompt(
  project: Project,
  test: TestCase,
  outcome: { reason: string; explanation: string | null; fix: string | null; sides: Sides },
): string {
  const { what, only, keep } = target(project, test.against);
  const title =
    test.against === "spec"
      ? "Fix the Piton specification so that this statement is true:"
      : test.against === "code"
        ? "Fix the code so that this statement is true:"
        : "Fix the Piton specification and the code so that this statement is true in both:";
  const lines = [title, "", test.expects, ""];
  lines.push(
    `The test \`${test.name}\` in \`${relative(test.file, project.root)}\` examines the statement in ${what}. It failed.`,
    `Reason: ${outcome.reason || "The agent gave no reason."}`,
  );
  if (outcome.sides !== null) {
    const state = (side: Side) => (side.pass ? "true" : "not true");
    lines.push(
      `Specification: the statement is ${state(outcome.sides.spec)}. ${outcome.sides.spec.reason}`.trimEnd(),
      `Code: the statement is ${state(outcome.sides.code)}. ${outcome.sides.code.reason}`.trimEnd(),
    );
  }
  if (outcome.explanation !== null) lines.push(`Explanation: ${outcome.explanation}`);
  lines.push("", "What to change:");
  if (outcome.fix !== null) {
    lines.push(outcome.fix);
  } else if (test.against === "both") {
    lines.push("Find where the specification and the code do not make the statement true. Change them so that both do.");
  } else {
    lines.push(`Find where ${only} does not make the statement true. Change it so that it does.`);
  }
  lines.push("", "Rules:");
  if (keep !== "") lines.push(`- Change only ${only}. ${keep}`);
  else lines.push("- Change the specification and the code. After the fix, they must agree.");
  lines.push("- Do not change the test.");
  if (test.against !== "code") {
    lines.push("- Change the specification in its `.pi` source. Do not edit generated files. Then run `piton check` and `piton build`.");
  } else {
    lines.push("- Do not edit generated files.");
  }
  lines.push(`- To examine the fix, run \`piton test ${test.name} --against ${test.against}\`.`);
  return lines.join("\n");
}
