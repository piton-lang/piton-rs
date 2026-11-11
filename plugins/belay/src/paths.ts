// This module holds helpers for paths.
//
// A path is a string with forward slashes. The helpers compare paths by their
// components, as the compiler does. Thus `a/./b`, `a//b` and `a/b/` are the
// same path as `a/b`. A path that starts with `/` is absolute.

import { compareText } from "./text";

interface Parts {
  absolute: boolean;
  parts: string[];
}

function split(path: string): Parts {
  const absolute = path.startsWith("/");
  return { absolute, parts: path.split("/").filter((part) => part !== "" && part !== ".") };
}

function show({ absolute, parts }: Parts): string {
  const joined = parts.join("/");
  return absolute ? `/${joined}` : joined;
}

/** The path in its plain form: one slash between components. */
export function clean(path: string): string {
  return show(split(path));
}

/** Joins `child` to `base`. An absolute `child` replaces `base`. */
export function join(base: string, child: string): string {
  if (child.startsWith("/")) return clean(child);
  const left = split(base);
  return show({ absolute: left.absolute, parts: [...left.parts, ...split(child).parts] });
}

/** Removes `.` and resolves `..` without the file system. A `..` that has
 * nothing to remove stays, as the compiler keeps it. */
export function normalize(path: string): string {
  const { absolute, parts } = split(path);
  const out: string[] = [];
  for (const part of parts) {
    if (part === "..") {
      // The compiler removes the last component here, even when it is `..`.
      if (out.length > 0) out.pop();
      else out.push("..");
    } else {
      out.push(part);
    }
  }
  return show({ absolute, parts: out });
}

/** The directory that contains `path`, or null for a root or an empty path. */
export function parent(path: string): string | null {
  const { absolute, parts } = split(path);
  if (parts.length === 0) return null;
  return show({ absolute, parts: parts.slice(0, -1) });
}

/** The directory that contains `path`, or the empty path. */
export function directory(path: string): string {
  return parent(path) ?? "";
}

/** The last component of `path`, or null. */
export function fileName(path: string): string | null {
  const { parts } = split(path);
  const last = parts[parts.length - 1];
  return last === undefined || last === ".." ? null : last;
}

function splitName(name: string): [string, string | null] {
  if (name === "..") return [name, null];
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return [name, null];
  return [name.slice(0, dot), name.slice(dot + 1)];
}

/** The last component without its extension. */
export function fileStem(path: string): string | null {
  const name = fileName(path);
  return name === null ? null : splitName(name)[0];
}

/** The extension of the last component, without its dot. */
export function extension(path: string): string | null {
  const name = fileName(path);
  return name === null ? null : splitName(name)[1];
}

/** Replaces the extension of the last component. */
export function withExtension(path: string, ext: string): string {
  const name = fileName(path);
  if (name === null) return path;
  const stem = splitName(name)[0];
  const directory = parent(path) ?? "";
  const replaced = ext === "" ? stem : `${stem}.${ext}`;
  return directory === "" && !path.startsWith("/") ? replaced : join(directory, replaced);
}

/** The rest of `path` after `prefix`, or null when `prefix` is not a
 * leading part of it. */
export function stripPrefix(path: string, prefix: string): string | null {
  const whole = split(path);
  const start = split(prefix);
  if (whole.absolute !== start.absolute || start.parts.length > whole.parts.length) return null;
  for (let i = 0; i < start.parts.length; i += 1) {
    if (start.parts[i] !== whole.parts[i]) return null;
  }
  return whole.parts.slice(start.parts.length).join("/");
}

/** Gives true if `prefix` is a leading part of `path`. */
export function startsWith(path: string, prefix: string): boolean {
  return stripPrefix(path, prefix) !== null;
}

/** Gives true if `suffix` is a trailing part of `path`. */
export function endsWith(path: string, suffix: string): boolean {
  const whole = split(path).parts;
  const end = split(suffix).parts;
  if (end.length > whole.length) return false;
  return end.every((part, i) => whole[whole.length - end.length + i] === part);
}

/** Gives true if `path` is absolute. */
export function isAbsolute(path: string): boolean {
  return path.startsWith("/");
}

/** Gives true if a component of `path` is `..`. */
export function leavesUpward(path: string): boolean {
  return split(path).parts.includes("..");
}

/** `path`, then each directory above it, up to the empty path or the root. */
export function ancestors(path: string): string[] {
  const out: string[] = [];
  let current: string | null = clean(path);
  while (current !== null) {
    out.push(current);
    current = parent(current);
  }
  return out;
}

/** Compares two paths by their components, as the compiler sorts them. */
export function comparePaths(a: string, b: string): number {
  const left = split(a);
  const right = split(b);
  if (left.absolute !== right.absolute) return left.absolute ? -1 : 1;
  const shared = Math.min(left.parts.length, right.parts.length);
  for (let i = 0; i < shared; i += 1) {
    const order = compareText(left.parts[i], right.parts[i]);
    if (order !== 0) return order;
  }
  return left.parts.length - right.parts.length;
}

/** `path` relative to `root` when it is inside it, else `path` itself. */
export function underRoot(path: string, root: string): string {
  return stripPrefix(path, root) ?? path;
}
