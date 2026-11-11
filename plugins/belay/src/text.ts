// This module holds helpers for text.
//
// The helpers do what the string methods of the compiler do. JavaScript
// strings are UTF-16, and its methods differ from the compiler in small ways.
// For example, `trim` removes a different set of characters, and `length`
// counts UTF-16 units, not bytes or characters. Belay uses these helpers, so
// that its output is the same as the output of the compiler.

/** The characters that the compiler counts as white space. */
const SPACE = "\t\n\u000b\f\r \u0085   -     　";
const LEADING = new RegExp(`^[${SPACE}]+`, "u");
const TRAILING = new RegExp(`[${SPACE}]+$`, "u");
const ANY_SPACE = new RegExp(`[${SPACE}]`, "u");

/** Removes white space at the start and at the end of `text`. */
export function trim(text: string): string {
  return trimEnd(trimStart(text));
}

/** Removes white space at the start of `text`. */
export function trimStart(text: string): string {
  return text.replace(LEADING, "");
}

/** Removes white space at the end of `text`. */
export function trimEnd(text: string): string {
  return text.replace(TRAILING, "");
}

/** Gives true if `text` contains a white space character. */
export function hasSpace(text: string): boolean {
  return ANY_SPACE.test(text);
}

/** Gives true if `text` is empty or holds only white space. */
export function isBlank(text: string): boolean {
  return trim(text) === "";
}

/** Splits `text` into lines. A line ends at `\n` or at `\r\n`. A final empty
 * line is not a line. */
export function lines(text: string): string[] {
  if (text === "") return [];
  const parts = text.split("\n");
  const ended = text.endsWith("\n");
  if (ended) parts.pop();
  return parts.map((line, index) =>
    (index < parts.length - 1 || ended) && line.endsWith("\r") ? line.slice(0, -1) : line,
  );
}

/** The number of bytes that `text` has in UTF-8. */
export function byteLength(text: string): number {
  let bytes = 0;
  for (const char of text) {
    const code = char.codePointAt(0)!;
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
  }
  return bytes;
}

/** The bytes of `text` in UTF-8. */
export function utf8(text: string): number[] {
  const out: number[] = [];
  for (const char of text) {
    const code = char.codePointAt(0)!;
    if (code < 0x80) {
      out.push(code);
    } else if (code < 0x800) {
      out.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    } else if (code < 0x10000) {
      out.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    } else {
      out.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    }
  }
  return out;
}

/** The number of characters (code points) in `text`. */
export function charCount(text: string): number {
  let count = 0;
  for (const _ of text) count += 1;
  return count;
}

/** Compares two strings by their code points, which is the order of their
 * UTF-8 bytes. */
export function compareText(a: string, b: string): number {
  if (a === b) return 0;
  const left = [...a];
  const right = [...b];
  const shared = Math.min(left.length, right.length);
  for (let i = 0; i < shared; i += 1) {
    const x = left[i].codePointAt(0)!;
    const y = right[i].codePointAt(0)!;
    if (x !== y) return x < y ? -1 : 1;
  }
  return left.length - right.length;
}

/** Counts the occurrences of `pattern` in `text` that do not overlap. */
export function occurrences(text: string, pattern: string): number {
  if (pattern === "") return charCount(text) + 1;
  return text.split(pattern).length - 1;
}

/** Formats a number as Piton writes it. A whole number has no fraction. */
export function formatNumber(n: number): string {
  if (Number.isNaN(n)) return "null";
  if (!Number.isFinite(n)) return n > 0 ? "Infinity" : "-Infinity";
  if (Number.isInteger(n) && Math.abs(n) < 1e15) return String(n);
  return String(n);
}
