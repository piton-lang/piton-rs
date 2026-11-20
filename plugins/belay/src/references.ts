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
    // The host walks the values. Only the references cross to the plugin,
    // not megabytes of text.
    const found: Occurrence[] = source.compilation.references(anchor).map((reference) => ({
      target: { anchor: reference.target.id, path: reference.target.path },
      from: reference.from,
      key: reference.key,
    }));
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
