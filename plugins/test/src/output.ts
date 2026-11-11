// This module sets the layout of the stdout output of `piton test`.
//
// Each result is a small block.
// The first line shows the status, the name of the test, and its duration.
// The duration is at the right edge.
// A dim line shows the file of the test and its tags.
// Below that line, the reason of the agent shows on one line.
// For a failure, the ASD-STE100 explanation shows on one line below it.
// These lines are not wrapped and have no bar, so the terminal wraps them and
// a user can select and copy each as coherent text.
// The status is a mark and a word.
// Thus, output without colour gives the same information.

import type { Context, Style } from "@piton/plugin";
import type { Sides } from "./judge";

/** The indent of the reason and the details. It aligns them after `✓ pass  `. */
const INDENT = 9;
/** The maximum width of a block. A wider terminal does not change it. */
const MAX_WIDTH = 100;

export type Status = "pass" | "fail" | "error";

const MARKS: Record<Status, [string, string, Style]> = {
  pass: ["✓", "pass", "success"],
  fail: ["✗", "FAIL", "error"],
  error: ["!", "error", "warning"],
};

/** Gives the number of characters in a string. It counts code points. */
export function length(text: string): number {
  return [...text].length;
}

/** Puts `text` on one line. If the line is longer than `width`, it cuts the
 * line and adds an ellipsis. */
export function truncate(text: string, width: number): string {
  const line = text.split(/\s+/).filter((word) => word.length > 0).join(" ");
  const chars = [...line];
  if (chars.length <= width) return line;
  if (width <= 0) return "";
  return chars.slice(0, width - 1).join("") + "…";
}

/** Shows a duration as minutes and seconds, for example 1:07. */
export function clock(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** Gives the width for the layout. This is the width of the terminal, up to
 * MAX_WIDTH. */
export function width(ctx: Context): number {
  const columns = ctx.terminal.stdout ? ctx.terminal.width : null;
  return Math.min(Math.max(columns ?? MAX_WIDTH, 40), MAX_WIDTH);
}

/** Breaks `text` into lines of `width` characters or less. It breaks the
 * lines at spaces. A word that is longer than the width goes on a line of its
 * own. It does not cut the word. */
export function wrap(text: string, width: number): string[] {
  const lines: string[] = [];
  for (const paragraph of text.split("\n")) {
    let line = "";
    for (const word of paragraph.split(/\s+/).filter((word) => word.length > 0)) {
      if (line.length > 0 && length(line) + 1 + length(word) > width) {
        lines.push(line);
        line = "";
      }
      line = line.length > 0 ? `${line} ${word}` : word;
    }
    if (line.length > 0) lines.push(line);
  }
  return lines;
}

/** Puts `text` on one line. It joins its lines and runs of spaces with one
 * space. */
export function oneLine(text: string): string {
  return text.split(/\s+/).filter((word) => word.length > 0).join(" ");
}

/** Gives the file of a test and its tags, for the dim detail line. */
export function details(file: string, tags: string[]): string {
  return tags.length === 0 ? file : `${file} · ${tags.join(", ")}`;
}

/** Makes the block for one test that finished. */
export function result(
  ctx: Context,
  status: Status,
  name: string,
  detail: string,
  elapsed: string,
  reason: string,
  explanation: string | null,
  sides: Sides,
  width: number,
): string {
  const [mark, word, style] = MARKS[status];
  const head = `${mark} ${word.padEnd(5)}  `;
  const room = Math.max(width - (INDENT + elapsed.length + 1), 8);
  const shown = truncate(name, room);
  const gap = Math.max(width - (INDENT + length(shown) + elapsed.length), 1);
  const pad = " ".repeat(INDENT);

  let out = `${ctx.paint(style, head)}${ctx.paint("heading", shown)}${" ".repeat(gap)}${ctx.paint("dim", elapsed)}\n`;
  out += `${pad}${ctx.paint("dim", detail)}\n`;
  // A test against both shows the result in the specification and in the
  // code. The test fails when one of them fails.
  if (sides !== null) {
    for (const [label, side] of [["spec", sides.spec], ["code", sides.code]] as const) {
      const [sideMark, sideWord, sideStyle] = MARKS[side.pass ? "pass" : "fail"];
      const text = side.reason.trim().length === 0 ? "(no reason given)" : oneLine(side.reason);
      out += `${pad}${label} ${ctx.paint(sideStyle, `${sideMark} ${sideWord.padEnd(4)}`)}  ${text}\n`;
    }
  }
  // The reason and the explanation are each one line. The terminal wraps a
  // long line, so the text that a user selects has no breaks, indents or bars.
  out += `${pad}${reason.trim().length === 0 ? "(no reason given)" : oneLine(reason)}\n`;
  // The explanation of a failure is in ASD-STE100. It shows below the reason,
  // in the colour of the status.
  if (explanation !== null && explanation.trim().length > 0) {
    out += `${pad}${ctx.paint(style, oneLine(explanation))}\n`;
  }
  return out;
}

/** Shows again the tests that did not pass, after all the results. Thus,
 * these tests do not go out of view above a long output. */
export function recap(ctx: Context, entries: [Status, string, string, Sides][]): string {
  if (entries.length === 0) return "";
  const nameWidth = Math.max(...entries.map(([, name]) => length(name)));
  let out = `${ctx.paint("heading", "Not passing")}\n`;
  for (const [status, name, file, sides] of entries) {
    const [mark, word, style] = MARKS[status];
    // For a test against both, tell which side failed.
    const where =
      sides === null
        ? ""
        : ` ${ctx.paint("dim", `(spec ${sides.spec.pass ? "pass" : "FAIL"}, code ${sides.code.pass ? "pass" : "FAIL"})`)}`;
    out += `  ${ctx.paint(style, `${mark} ${word.padEnd(5)}`)}  ${name.padEnd(nameWidth)}  ${ctx.paint("dim", file)}${where}\n`;
  }
  return out;
}

/** Shows the fix prompt of each failed test, after the recap. The prompts are
 * not wrapped and not indented. Thus, a user can copy each prompt to an agent
 * without changes. A rule above each prompt gives the name of the test. */
export function fixPrompts(ctx: Context, prompts: [string, string][], width: number): string {
  if (prompts.length === 0) return "";
  let out = `${ctx.paint("heading", "Fix prompts")}\n`;
  out += `${ctx.paint("dim", "Give a prompt to an agent to fix its test.")}\n`;
  for (const [name, prompt] of prompts) {
    const label = `── ${name} `;
    const rule = label + "─".repeat(Math.max(width - length(label), 3));
    out += `\n${ctx.paint("error", rule)}\n${prompt}\n`;
  }
  return out;
}

/** One test, for `--list`. */
export interface Listed {
  name: string;
  file: string;
  against: string;
  tags: string[];
  expects: string;
}

/** Shows the tests that a run will include. It puts each test below the file
 * of that test. */
export function list(ctx: Context, tests: Listed[], width: number): string {
  const files = new Map<string, Listed[]>();
  for (const test of tests) {
    const group = files.get(test.file) ?? [];
    group.push(test);
    files.set(test.file, group);
  }
  const blocks: string[] = [];
  for (const file of [...files.keys()].sort()) {
    let out = `${ctx.paint("name", file)}\n`;
    for (const test of files.get(file)!) {
      let meta = `against ${test.against}`;
      if (test.tags.length > 0) meta += ` · ${test.tags.join(", ")}`;
      out += `  ${ctx.paint("heading", test.name)}  ${ctx.paint("dim", meta)}\n`;
      for (const line of wrap(test.expects, width - 4)) out += `    ${line}\n`;
    }
    blocks.push(out);
  }
  return blocks.join("\n");
}
