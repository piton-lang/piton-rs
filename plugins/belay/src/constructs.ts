// This module recognizes the four constructs.
//
// A construct is an ordinary anchor. An anchor is a skill when its
// inheritance chain reaches the `Skill` anchor of Belay. Thus Belay asks the
// compilation which anchors extend each construct, and does not look at
// keywords.

import type { Value } from "@piton/plugin";
import type { Source } from "./source";
import { compareText } from "./text";

/** Which of the four constructs an anchor is. The order of the list is the
 * order of the output. */
export type ConstructKind = "instruction" | "skill" | "command" | "agent";

const ORDER: ConstructKind[] = ["instruction", "skill", "command", "agent"];

/** The properties that have a defined place in the artifact. The remainder
 * does not repeat them. */
export function primaryProperties(kind: ConstructKind): string[] {
  switch (kind) {
    case "instruction":
      return ["description", "prompt"];
    case "skill":
      return ["description", "prompt", "useWhen", "files"];
    case "command":
      return ["description", "prompt"];
    case "agent":
      return ["description", "role", "prompt", "file"];
  }
}

/** The properties that become metadata of the target, where a reference has
 * no representation. */
export function metadataProperties(kind: ConstructKind): string[] {
  switch (kind) {
    case "instruction":
      return [];
    case "skill":
      return ["description", "useWhen"];
    case "command":
    case "agent":
      return ["description"];
  }
}

/** A recognized construct, with its resolved properties. */
export interface Construct {
  anchor: number;
  kind: ConstructKind;
  name: string;
  /** The properties in their order, with nothing flattened. */
  properties: [string, Value][];
}

/** The value of one property of a construct, or undefined. */
export function property(item: Construct, key: string): Value | undefined {
  return item.properties.find(([name]) => name === key)?.[1];
}

/** A text property as plain text for metadata, where a Markdown link is not
 * read as a link. A reference becomes the name of its target. */
export function metadataText(item: Construct, key: string): string | null {
  const value = property(item, key);
  return value?.kind === "text" ? value.text : null;
}

/** The names of the properties without a defined place, less the native
 * options that the target uses. */
export function remainder(item: Construct, consumed: string[]): string[] {
  const primary = primaryProperties(item.kind);
  return item.properties
    .map(([name]) => name)
    .filter((name) => !primary.includes(name) && !consumed.includes(name));
}

/** The anchors that extend each construct of Belay. */
export class Constructs {
  private readonly kinds = new Map<number, ConstructKind>();

  constructor(source: Source) {
    // An anchor that extends one construct extends no other, except the
    // abstract Construct that all four extend. The order is the order of
    // the compiler, so the first match wins.
    for (const kind of ["skill", "command", "agent", "instruction"] as ConstructKind[]) {
      for (const id of source.extending(kind[0].toUpperCase() + kind.slice(1))) {
        if (!this.kinds.has(id)) this.kinds.set(id, kind);
      }
    }
  }

  /** Which construct an anchor is, or null. */
  classify(anchor: number): ConstructKind | null {
    return this.kinds.get(anchor) ?? null;
  }

  /** Every concrete construct among `anchors`, sorted by kind, then name. */
  collect(source: Source, anchors: number[]): Construct[] {
    const found: Construct[] = [];
    for (const anchor of anchors) {
      const info = source.anchor(anchor);
      const kind = this.classify(anchor);
      if (info.abstract || kind === null) continue;
      found.push({ anchor, kind, name: info.name, properties: source.values(anchor) });
    }
    return found.sort(
      (a, b) => ORDER.indexOf(a.kind) - ORDER.indexOf(b.kind) || compareText(a.name, b.name),
    );
  }
}
