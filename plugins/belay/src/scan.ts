// This module reads the text of planned files and of configuration files.
//
// The checks of the plan read headings, links and metadata from the files
// that Belay plans. They also read a few fields from JSON and TOML files.
// Each reader here reads only what Belay needs, as the compiler reads it.

import { headingSlug } from "./names";
import { lines, trim, trimStart, utf8 } from "./text";

/** Calls `visit` with the start and the end of each line of `text`, as
 * `lines` splits it, without a copy of each line. A `\r` before a line
 * break is not part of the line. */
function eachLine(text: string, visit: (start: number, end: number) => void): void {
  let start = 0;
  while (start < text.length) {
    let next = text.indexOf("\n", start);
    const broken = next >= 0;
    if (!broken) next = text.length;
    const end = broken && next > start && text.charCodeAt(next - 1) === 13 ? next - 1 : next;
    visit(start, end);
    start = next + 1;
  }
}

/** The next index of `pattern` at or after `from`, as a position that only
 * moves forward. Each line asks only when it passed the last position. */
function finder(text: string, pattern: string): (from: number) => number {
  let found = -2;
  return (from) => {
    if (found !== -1 && found < from) found = text.indexOf(pattern, from);
    return found;
  };
}

/** The ATX headings of a Markdown text, with their levels. Headings in
 * fenced code are not headings.
 *
 * A plan holds megabytes of Markdown, and few lines are headings or fences.
 * Thus the scan looks only at the lines that start with `#` or that hold a
 * fence. */
export function scanHeadings(text: string): [number, string][] {
  const out: [number, string][] = [];
  let fence: string | null = null;
  const ticks = finder(text, "```");
  const tildes = finder(text, "~~~");
  eachLine(text, (start, end) => {
    const tick = ticks(start);
    const tilde = tildes(start);
    const fenced = (tick >= 0 && tick < end) || (tilde >= 0 && tilde < end);
    if (fenced) {
      const trimmed = trimStart(text.slice(start, end));
      const marker = trimmed.startsWith("```") ? "```" : trimmed.startsWith("~~~") ? "~~~" : null;
      if (marker !== null) {
        if (fence === marker) fence = null;
        else if (fence === null) fence = marker;
        return;
      }
    }
    if (fence !== null || text.charCodeAt(start) !== 35 || start >= end) return;
    const line = text.slice(start, end);
    let level = 0;
    while (line[level] === "#") level += 1;
    if (level >= 1 && level <= 6) {
      const rest = line.slice(level);
      if (rest === "" || rest.startsWith(" ")) {
        out.push([level, trim(trim(rest).replace(/#+$/, ""))]);
      }
    }
  });
  return out;
}

/** Every fragment that one Markdown document defines. A repeated slug gets
 * `-1`, `-2` and so on, as a Markdown viewer numbers it. */
export function slugs(text: string): Set<string> {
  const counts = new Map<string, number>();
  const out = new Set<string>();
  for (const [, title] of scanHeadings(text)) {
    const base = headingSlug(title);
    const count = counts.get(base) ?? 0;
    out.add(count === 0 ? base : `${base}-${count}`);
    counts.set(base, count + 1);
  }
  return out;
}

/** The targets of the inline Markdown links outside fenced code.
 *
 * The plan of a large project holds megabytes of Markdown. Thus the search
 * uses `indexOf`, and looks only at the lines that hold a link or a fence. */
export function markdownLinks(contents: string): string[] {
  const out: string[] = [];
  let inFence = false;
  const ticks = finder(contents, "```");
  const links = finder(contents, "](");
  eachLine(contents, (start, end) => {
    const tick = ticks(start);
    if (tick >= 0 && tick < end && trimStart(contents.slice(start, end)).startsWith("```")) {
      inFence = !inFence;
      return;
    }
    if (inFence) return;
    const first = links(start);
    if (first < 0 || first >= end) return;
    const line = contents.slice(start, end);
    let open = first - start;
    while (open >= 0) {
      const close = line.indexOf(")", open + 2);
      if (close < 0) break;
      const target = line.slice(open + 2, close);
      if (target !== "" && !target.includes(" ")) {
        out.push(target);
        open = line.indexOf("](", close + 1);
      } else {
        open = line.indexOf("](", open + 1);
      }
    }
  });
  return out;
}

/** Gives true if a link starts with a URL scheme, like `https:`. */
export function hasScheme(link: string): boolean {
  const colon = link.indexOf(":");
  if (colon < 0) return false;
  const scheme = link.slice(0, colon);
  return scheme !== "" && !/[/#.]/.test(scheme) && /^[A-Za-z0-9+-]+$/.test(scheme);
}

/** A scalar field of a YAML frontmatter block. Quotes around it are removed;
 * nothing else is decoded. */
export function frontmatterField(contents: string, field: string): string | null {
  if (!contents.startsWith("---\n")) return null;
  const rest = contents.slice(4);
  const end = rest.indexOf("\n---");
  if (end < 0) return null;
  for (const line of lines(rest.slice(0, end))) {
    if (line.startsWith(`${field}: `)) {
      const value = trim(line.slice(field.length + 2));
      return value.length >= 2 && value.startsWith('"') && value.endsWith('"') ? value.slice(1, -1) : value;
    }
  }
  return null;
}

/** The `name =` or `description =` of a TOML document's top level. */
export function tomlField(contents: string, field: string): string | null {
  for (const line of lines(contents)) {
    if (trimStart(line).startsWith("[")) break;
    const equals = line.indexOf("=");
    if (equals < 0) continue;
    if (trim(line.slice(0, equals)) !== field) continue;
    const value = trim(line.slice(equals + 1));
    for (const quote of ['"', "'"]) {
      if (value.length >= 2 && value.startsWith(quote) && value.endsWith(quote)) return value.slice(1, -1);
    }
    return value;
  }
  return null;
}

/** The strings of the first JSON array stored under `key`. The reader is
 * small and forgiving, as the reader of the compiler is. */
export function stringArray(document: string, key: string): string[] {
  const quoted = `"${key}"`;
  const start = document.indexOf(quoted);
  if (start < 0) return [];
  const rest = document.slice(start + quoted.length);
  if (!trimStart(rest).startsWith(":")) return [];
  const open = rest.indexOf("[");
  if (open < 0) return [];
  const chars = [...rest.slice(open + 1)];
  const out: string[] = [];
  let i = 0;
  const next = (): string | undefined => chars[i++];
  for (let ch = next(); ch !== undefined; ch = next()) {
    if (ch === "]") break;
    if (ch !== '"') continue;
    let value = "";
    for (let inner = next(); inner !== undefined; inner = next()) {
      if (inner === '"') break;
      if (inner !== "\\") {
        value += inner;
        continue;
      }
      const escaped = next();
      if (escaped === undefined) break;
      if (escaped === "n") value += "\n";
      else if (escaped === "t") value += "\t";
      else if (escaped === "r") value += "\r";
      else if (escaped === "u") {
        const code = chars.slice(i, i + 4).join("");
        i += Math.min(4, chars.length - i);
        const parsed = /^[0-9A-Fa-f]{1,4}$/.test(code) ? parseInt(code, 16) : NaN;
        if (!Number.isNaN(parsed) && (parsed < 0xd800 || parsed > 0xdfff)) value += String.fromCodePoint(parsed);
      } else value += escaped;
    }
    out.push(value);
  }
  return out;
}

/** Matches a path against a pattern. `*` and `?` stay in one path segment,
 * and `**` crosses segments. A leading `./` of the pattern is ignored. */
export function globMatches(pattern: string, path: string): boolean {
  let p = pattern;
  while (p.startsWith("./")) p = p.slice(2);
  const pb = utf8(p);
  const sb = utf8(path);
  const SLASH = 0x2f;
  const STAR = 0x2a;
  const go = (pi: number, si: number): boolean => {
    if (pi >= pb.length) return si >= sb.length;
    const c = pb[pi];
    if (c === STAR && pb[pi + 1] === STAR) {
      const rest = pb[pi + 2] === SLASH ? pi + 3 : pi + 2;
      for (let i = si; i <= sb.length; i += 1) if (go(rest, i)) return true;
      return false;
    }
    if (c === STAR) {
      for (let i = si; i <= sb.length; i += 1) {
        if (i !== si && sb[i - 1] === SLASH) break;
        if (go(pi + 1, i)) return true;
      }
      return false;
    }
    if (c === 0x3f) return si < sb.length && sb[si] !== SLASH && go(pi + 1, si + 1);
    return si < sb.length && sb[si] === c && go(pi + 1, si + 1);
  };
  return go(0, 0);
}
