// This module finds the anchors that the reference tree holds, and where
// each document of the tree goes.
//
// A reference, `@{Anchor}` or `@{Anchor.property}`, links to the compiled
// form of its target. It does not copy the target. Thus Belay compiles each
// anchor that an emitted artifact references into the reference tree, and
// it does this again for the references of those anchors. A link then always
// has a destination.
//
// The reference tree mirrors the source tree: one file for each source
// module, with a heading for each anchor in it.

import type { Value } from "@piton/plugin";
import { join, normalize, startsWith, stripPrefix, withExtension } from "./paths";
import type { Source } from "./source";

/** A reference: an anchor, and the property path in it. */
export interface Ref {
  anchor: number;
  path: string[];
}

/** One place where a reference is written. */
export interface Occurrence {
  target: Ref;
  /** The anchor whose property holds the reference. */
  from: number;
  /** That property. */
  key: string;
}

/** Why a referenced anchor has no compiled form. */
export type Unrepresentable = "abstract" | "package" | "outside-project";

/** The anchors that the emitted artifacts reach through references. */
export interface Closure {
  /** Anchors that get a reference document. */
  referenced: Set<number>;
  /** Each reference in the emitted content, in the order found. */
  occurrences: Occurrence[];
  /** Referenced anchors that Belay cannot compile, and why. */
  unrepresentable: Map<number, Unrepresentable>;
}

/** The text of a reference as the author wrote it: `Name.path`. */
export function display(source: Source, target: Ref): string {
  return [source.name(target.anchor), ...target.path].join(".");
}

/** Follows references from `roots` until it finds no new anchor. The
 * content of an anchor that a value embeds (`{Foo}`) is part of the anchor
 * that embeds it, so Belay follows its references from there too. */
export function close(source: Source, roots: number[]): Closure {
  const closure: Closure = { referenced: new Set(), occurrences: [], unrepresentable: new Map() };
  const visited = new Set<number>();
  const queue = [...roots].reverse();
  while (queue.length > 0) {
    const anchor = queue.pop()!;
    if (visited.has(anchor)) continue;
    visited.add(anchor);
    const found: Occurrence[] = [];
    const embedded = new Set<number>([anchor]);
    for (const [key, value] of source.values(anchor)) {
      occurrencesIn(source, value, anchor, key, found, embedded);
    }
    for (const occurrence of found) {
      const reached = occurrence.target.anchor;
      closure.occurrences.push(occurrence);
      const reason = unrepresentable(source, reached);
      if (reason === null) {
        closure.referenced.add(reached);
        if (!visited.has(reached)) queue.push(reached);
      } else {
        closure.unrepresentable.set(reached, reason);
      }
    }
  }
  return closure;
}

/** Collects the references in a value, each with the property of the anchor
 * that holds it. It goes into embedded anchors, so a reference in an inlined
 * copy belongs to the place where the author wrote it. */
function occurrencesIn(
  source: Source,
  value: Value | { kind: "entry"; key: string; value: Value },
  from: number,
  key: string,
  out: Occurrence[],
  embedded: Set<number>,
): void {
  switch (value.kind) {
    case "reference":
      out.push({ target: { anchor: value.id, path: value.path }, from, key });
      return;
    case "text":
      for (const target of value.references) out.push({ target: { anchor: target.id, path: target.path }, from, key });
      return;
    case "list":
      for (const item of value.items) occurrencesIn(source, item, from, key, out, embedded);
      return;
    case "dictionary":
      for (const [, item] of value.entries) occurrencesIn(source, item, from, key, out, embedded);
      return;
    case "mixed":
      for (const item of value.items) occurrencesIn(source, item, from, key, out, embedded);
      return;
    case "entry":
      occurrencesIn(source, value.value, from, key, out, embedded);
      return;
    case "anchor":
      if (!embedded.has(value.id)) {
        embedded.add(value.id);
        for (const [inner, item] of source.values(value.id)) {
          occurrencesIn(source, item, value.id, inner, out, embedded);
        }
      }
      return;
    default:
      return;
  }
}

function unrepresentable(source: Source, anchor: number): Unrepresentable | null {
  const record = source.anchor(anchor);
  if (record.abstract) return "abstract";
  if (record.file.startsWith("@")) return "package";
  if (!startsWith(record.file, source.project.root)) return "outside-project";
  return null;
}

/** Where the reference document of a module goes for one target. A module
 * under `shapeRoot` keeps its place under the compiled shape root, so the
 * shape tree mirrors the architecture. Any other module keeps its place
 * under the source root, or else under the project root, beneath the
 * reference root. */
export function documentPath(
  sourcePath: string,
  sourceRoot: string,
  projectRoot: string,
  shapeRoot: string | null,
  referenceRoot: string,
  compiledShapeRoot: string,
): string {
  const inShape = shapeRoot === null ? null : stripPrefix(sourcePath, shapeRoot);
  const [base, relative] =
    inShape !== null
      ? [compiledShapeRoot, inShape]
      : [
          referenceRoot,
          stripPrefix(sourcePath, sourceRoot) ?? stripPrefix(sourcePath, projectRoot) ?? sourcePath,
        ];
  return normalize(withExtension(join(base, relative), "md"));
}
