// This module maps the shape tree onto the source tree of the application.
//
// The shape tree mirrors the source tree approximately. An instruction
// attaches to the closest implementation scope that exists. If the matching
// directory under `codeRoot` does not exist, placement goes up until it finds
// one. It stops at `codeRoot`.

import { join, normalize, parent, stripPrefix } from "./paths";

/** The directory under `codeRoot` that an instruction's guidance goes to,
 * or null when not even `codeRoot` exists. */
export function place(
  sourcePath: string,
  shapeRoot: string,
  codeRoot: string,
  exists: (path: string) => boolean,
): string | null {
  const relative = stripPrefix(sourcePath, shapeRoot);
  if (relative === null) return null;
  let directory = parent(relative) ?? "";
  for (;;) {
    const candidate = normalize(join(codeRoot, directory));
    if (exists(candidate)) return candidate;
    if (directory === "") return null;
    directory = parent(directory) ?? "";
  }
}

/** Gives true if `sourcePath` is under `shapeRoot`. */
export function isShapeSource(sourcePath: string, shapeRoot: string): boolean {
  return stripPrefix(sourcePath, shapeRoot) !== null;
}
