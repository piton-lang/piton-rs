// This module makes names from the names of anchors.
//
// A name splits into words at a lowercase letter or a digit before an
// uppercase letter, and at `-`, `_` and spaces. A run of uppercase letters
// stays one word. Thus `whatIsAType` becomes the title `What Is AType`.
// The rules are the rules of the compiler.

import { trim } from "./text";

const LOWER = /^\p{Lowercase}$/u;
const UPPER = /^\p{Uppercase}$/u;
const ASCII_DIGIT = /^[0-9]$/;
const ALPHANUMERIC = /^[\p{Alphabetic}\p{Nd}\p{Nl}\p{No}]$/u;

/** Splits a source name into its words. */
export function splitWords(name: string): string[] {
  const words: string[] = [];
  let current = "";
  const chars = [...name];
  chars.forEach((ch, i) => {
    if (ch === "-" || ch === "_" || ch === " ") {
      if (current !== "") words.push(current);
      current = "";
      return;
    }
    const previous = i === 0 ? null : chars[i - 1];
    const boundary =
      previous !== null &&
      (LOWER.test(previous) || ASCII_DIGIT.test(previous)) &&
      (UPPER.test(ch) || (ASCII_DIGIT.test(ch) && !ASCII_DIGIT.test(previous)));
    if (boundary && current !== "") {
      words.push(current);
      current = "";
    }
    current += ch;
  });
  if (current !== "") words.push(current);
  return words;
}

/** A source name as a title: `orderOfPrecedence` becomes `Order Of Precedence`. */
export function titleCase(name: string): string {
  const words = splitWords(name);
  if (words.length === 0) return name;
  return words
    .map((word) => {
      const [first, ...rest] = [...word];
      return first === undefined ? "" : first.toUpperCase() + rest.join("");
    })
    .join(" ");
}

/** A source name in kebab case: `BuildTooling` becomes `build-tooling`. */
export function kebabCase(name: string): string {
  const words = splitWords(name);
  if (words.length === 0) return name.replace(/[A-Z]/g, (c) => c.toLowerCase());
  return words
    .map((word) => word.toLowerCase())
    .filter((word) => word !== "")
    .join("-");
}

/** Gives true if `name` is 1 to 64 lowercase ASCII letters and digits, with
 * single hyphens between them. */
export function isValidArtifactName(name: string): boolean {
  return [...name].length <= 64 && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(name);
}

/** The fragment that a Markdown viewer gives a heading. */
export function headingSlug(title: string): string {
  let out = "";
  for (const ch of trim(title)) {
    if (ALPHANUMERIC.test(ch) || ch === "_" || ch === "-") {
      out += ch.toLowerCase();
    } else if (ch === " ") {
      out += "-";
    }
  }
  return out;
}
