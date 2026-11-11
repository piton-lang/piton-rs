// This module holds the rules that put construct content into the files of
// a target.
//
// The rules for the Markdown body are the same for each target. The targets
// differ in the frontmatter or the document format around the body. The
// host encodes frontmatter, YAML and TOML with the encoders of the compiler.

import type { Fields } from "@piton/plugin";
import { isBlank, trim, trimEnd } from "./text";

export type { Fields };

/** Joins a primary body and its serialized remainder. */
export function body(primary: string, remainder: string): string {
  const first = trimEnd(primary);
  const rest = trimEnd(remainder);
  if (first === "" && rest === "") return "";
  if (first === "") return `${rest}\n`;
  if (rest === "") return `${first}\n`;
  return `${first}\n\n${rest}\n`;
}

/** The introduction of an agent: `You are a`, then the role, as the
 * specification words it. */
export function roleIntroduction(role: string): string {
  return `You are a ${trim(role)}`;
}

/** The discovery description of a skill: the description, then `Use when`
 * and the `useWhen` text, with single spaces between them. */
export function discoveryDescription(description: string, useWhen: string): string {
  const parts = [trim(description), isBlank(useWhen) ? "" : `Use when ${trim(useWhen)}`];
  return parts.filter((part) => part !== "").join(" ");
}
