// This module holds the migrations.
//
// A migration finds one old way to write a project in the files of the
// project, as they are. It does not compile them, because a project that
// needs a migration often does not compile. It gives the full new contents of
// each file that it changes.

import type { Context, Finding } from "@piton/plugin";

/** The configuration file, at the project root. */
const CONFIG = "piton.config.pi";

/** The number of spaces before the text of a line. */
function indentOf(line: string): number {
  return line.length - line.trimStart().length;
}

/** The index after the last line of the block that starts at `at`: the lines
 * below it that are blank or more indented than it. Blank lines at the end
 * of the block are not part of it. */
function blockEnd(lines: string[], at: number): number {
  const indent = indentOf(lines[at]);
  let end = at + 1;
  let last = at + 1;
  while (end < lines.length) {
    const line = lines[end];
    if (line.trim().length > 0) {
      if (indentOf(line) <= indent) break;
      last = end + 1;
    }
    end += 1;
  }
  return last;
}

/** The text after the colon of a `key:` line, trimmed. */
function inlineValue(line: string): string {
  return line.slice(line.indexOf(":") + 1).trim();
}

/** Splits the items of an inline list `[a, b]` at the commas that are not in
 * braces, brackets or parentheses. A value that is not a list is one item. */
function inlineItems(value: string): string[] {
  if (!(value.startsWith("[") && value.endsWith("]"))) return value.length > 0 ? [value] : [];
  const inner = value.slice(1, -1);
  const items: string[] = [];
  let depth = 0;
  let current = "";
  for (const ch of inner) {
    if (ch === "{" || ch === "[" || ch === "(") depth += 1;
    if (ch === "}" || ch === "]" || ch === ")") depth -= 1;
    if (ch === "," && depth === 0) {
      items.push(current.trim());
      current = "";
    } else {
      current += ch;
    }
  }
  items.push(current.trim());
  return items.filter((item) => item.length > 0);
}

/** The item lines of the key at `at`, indented to `indent`. A block keeps its
 * lines, moved to the new indent. An inline list gives one `- item` line for
 * each item. */
function itemLines(lines: string[], at: number, indent: number): string[] {
  const value = inlineValue(lines[at]);
  if (value.length > 0) return inlineItems(value).map((item) => `${" ".repeat(indent)}- ${item}`);
  const block = lines.slice(at + 1, blockEnd(lines, at));
  const first = block.find((line) => line.trim().length > 0);
  if (first === undefined) return [];
  const shift = indent - indentOf(first);
  return block.map((line) => {
    if (line.trim().length === 0) return "";
    const own = Math.max(0, indentOf(line) + shift);
    return " ".repeat(own) + line.trimStart();
  });
}

/** Moves the items of `frameworks:` to `plugins:` in the text of a
 * configuration. Gives null when the text has no `frameworks:` key. When it
 * has one, the text that it gives never has the key: each key can hold its
 * items as a block or as an inline list. */
export function frameworksToPlugins(text: string): string | null {
  const lines = text.split("\n");
  const at = lines.findIndex((line) => /^\s+frameworks:/.test(line));
  if (at < 0) return null;
  const indent = indentOf(lines[at]);
  const isPlugins = (line: string) => indentOf(line) === indent && /^\s+plugins:/.test(line);
  const pluginsAt = lines.findIndex(isPlugins);

  // Without a plugins key, the frameworks key gets the new name.
  if (pluginsAt < 0) {
    lines[at] = lines[at].replace("frameworks:", "plugins:");
    return lines.join("\n");
  }

  // With the two keys, the items of frameworks go to the end of plugins.
  // An inline plugins list becomes a block first, so that items can follow.
  const pluginsBlock = lines.slice(pluginsAt + 1, blockEnd(lines, pluginsAt));
  const firstPlugin = pluginsBlock.find((line) => line.trim().length > 0);
  const itemIndent = firstPlugin === undefined ? indent + 4 : indentOf(firstPlugin);
  const moved = itemLines(lines, at, itemIndent);

  const end = inlineValue(lines[at]).length > 0 ? at + 1 : blockEnd(lines, at);
  lines.splice(at, end - at);
  // Remove a blank line that the removal leaves next to another one.
  if (at > 0 && at < lines.length && lines[at - 1].trim() === "" && lines[at].trim() === "") lines.splice(at, 1);

  const target = lines.findIndex(isPlugins);
  const present = itemLines(lines, target, itemIndent);
  const targetEnd = inlineValue(lines[target]).length > 0 ? target + 1 : blockEnd(lines, target);
  const key = lines[target].slice(0, lines[target].indexOf(":") + 1);
  lines.splice(target, targetEnd - target, key, ...present, ...moved);
  return lines.join("\n");
}

/** Every migration that the project needs. */
export function findings(ctx: Context): Finding[] {
  const out: Finding[] = [];
  if (ctx.stat(CONFIG)?.kind !== "file") return out;
  const config = ctx.readFile(CONFIG);

  const moved = frameworksToPlugins(config);
  if (moved !== null) {
    out.push({
      message: "piton.config.pi lists frameworks, which is now plugins: a framework is a plugin",
      edits: [{ path: CONFIG, contents: moved }],
    });
  }
  return out;
}
