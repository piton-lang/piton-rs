// This module handles the files that a skill or an agent brings from its own
// source tree.
//
// The `files` of a skill names a folder. Belay copies the folder next to the
// SKILL.md of the skill. If the folder has its own SKILL.md, that file is the
// skill, and Belay uses it as it is. The `file` of an agent names its native
// definition, which Belay also uses as it is. Each property is one path for
// all targets, or a dictionary from target id to path.
//
// A path is relative to the `.pi` file that declares the property. A path
// that starts with `/` is relative to the source root, as imports are.
// Belay copies bytes: it renders, links or rewrites nothing in a copy.

import type { Value } from "@piton/plugin";
import { ADAPTERS, adapterFor, type Target } from "./adapters";
import { type Construct, type ConstructKind } from "./constructs";
import { error, warning } from "./diagnostics";
import type { Copied, Draft } from "./draft";
import { join, normalize, parent, startsWith, stripPrefix } from "./paths";
import type { Site, Source } from "./source";
import { compareText, trim } from "./text";

/** The property that names the vendored files of a construct, if it can have one. */
export function vendoredProperty(kind: ConstructKind): string | null {
  switch (kind) {
    case "skill":
      return "files";
    case "agent":
      return "file";
    default:
      return null;
  }
}

/** One file of a vendored folder. */
export interface Entry {
  /** The path in the folder, with `/` between components. */
  relative: string;
  text: string;
  copied: Copied;
}

/** What the vendored files of a construct say about one target. */
export type Choice =
  /** Nothing is vendored: Belay renders the construct. */
  | { kind: "render" }
  /** Use the files at this absolute path. */
  | { kind: "path"; path: string }
  /** The dictionary gives null for the target, so the target gets no output. */
  | { kind: "omit" }
  /** The value has a problem, which Belay reported. */
  | { kind: "invalid" };

function find(item: Construct, key: string): Value | undefined {
  return item.properties.find(([name]) => name === key)?.[1];
}

/** The entries of a dictionary, or of a block that holds keys. */
function entries(value: Value): [string, Value][] {
  if (value.kind === "dictionary") return value.entries;
  if (value.kind === "mixed") {
    const out: [string, Value][] = [];
    for (const item of value.items) if (item.kind === "entry") out.push([item.key, item.value]);
    return out;
  }
  return [];
}

function plainText(value: Value): string | null {
  return value.kind === "text" && value.references.length === 0 ? value.text : null;
}

/** A path for messages: relative to the project root when it is inside it. */
export function shown(source: Source, path: string): string {
  return stripPrefix(path, source.project.root) ?? path;
}

/** What a construct names for `target`. */
export function choose(source: Source, item: Construct, target: Target, draft: Draft): Choice {
  const key = vendoredProperty(item.kind);
  if (key === null) return { kind: "render" };
  const value = find(item, key);
  if (value === undefined || value.kind === "null") return { kind: "render" };
  const site = source.propertySite(item.anchor, key);
  let written: string;
  const direct = plainText(value);
  if (direct !== null) {
    written = direct;
  } else if (value.kind === "dictionary" || value.kind === "mixed") {
    const chosen = entries(value).find(([id]) => id === target.id);
    if (chosen === undefined) {
      draft.diagnostics.push(
        error(
          "missing-vendored-target",
          `\`${item.name}.${key}\` names a path for some targets but not for ${target.id}, which this project builds`,
          site,
          { help: `add \`${target.id}: <path>\` under \`${key}\`, or \`${target.id}: null\` to leave it out of that target` },
        ),
      );
      return { kind: "invalid" };
    }
    const [, entry] = chosen;
    if (entry.kind === "null") return { kind: "omit" };
    const text = plainText(entry);
    if (text === null) {
      draft.diagnostics.push(
        error("invalid-vendored-path", `\`${key}.${target.id}\` on \`${item.name}\` must be a path`, site),
      );
      return { kind: "invalid" };
    }
    written = text;
  } else {
    draft.diagnostics.push(
      error(
        "invalid-vendored-path",
        `\`${key}\` on \`${item.name}\` must be a path, or a dictionary from target id to path`,
        site,
      ),
    );
    return { kind: "invalid" };
  }
  written = trim(written);
  const declaring = source.declaringModule(item.anchor, key);
  const path = normalize(
    written.startsWith("/")
      ? join(source.project.sourceRoot, written.slice(1))
      : join(parent(declaring) ?? "", written),
  );
  if (!startsWith(path, source.project.root)) {
    draft.diagnostics.push(
      error(
        "vendored-outside-project",
        `\`${written}\` on \`${item.name}\` resolves to \`${path}\`, outside the project`,
        site,
      ),
    );
    return { kind: "invalid" };
  }
  return { kind: "path", path };
}

/** Reports target ids in a dictionary of paths that no adapter has. Belay
 * does this one time for each construct, not for each target. */
export function checkTargets(source: Source, item: Construct, draft: Draft): void {
  const key = vendoredProperty(item.kind);
  if (key === null) return;
  const value = find(item, key);
  if (value === undefined) return;
  for (const [id] of entries(value)) {
    if (adapterFor(id) !== null) continue;
    draft.diagnostics.push(
      error("unknown-vendored-target", `\`${key}.${id}\` on \`${item.name}\` names no target`, source.propertySite(item.anchor, key), {
        help: `the targets are ${ADAPTERS.map((a) => `\`${a.targetId}\``).join(", ")}`,
      }),
    );
  }
}

/** Warns when a target does not use its own path, because it reads the copy
 * of the skill that another target writes. */
export function reportUnused(source: Source, item: Construct, target: Target, peer: Target, draft: Draft): void {
  const key = vendoredProperty(item.kind);
  if (key === null) return;
  const value = find(item, key);
  if (value === undefined) return;
  const all = entries(value);
  const pathOf = (id: string) => all.find(([name]) => name === id)?.[1];
  const own = pathOf(target.id);
  const theirs = pathOf(peer.id);
  if (own === undefined || theirs === undefined) return;
  if (JSON.stringify(own) === JSON.stringify(theirs)) return;
  draft.diagnostics.push(
    warning(
      "unused-vendored-files",
      `${target.anchorName} reads ${peer.anchorName}'s copy of the skill \`${item.name}\`, so the files \`${key}.${target.id}\` names are not written`,
      source.propertySite(item.anchor, key),
      { help: "set `crossDiscovery: separate` in the belay-config if each tool's tree is deployed apart" },
    ),
  );
}

/** The text of the error that reading `path` gives, as the system says it. */
function readError(source: Source, path: string): string | null {
  const status = source.stat(path);
  if (status === null) return "No such file or directory (os error 2)";
  if (status.kind === "directory") return "Is a directory (os error 21)";
  return null;
}

/** Reads one vendored file: its text, or nothing when it is not UTF-8 text. */
export function readFile(source: Source, path: string): { text: string; copied: Copied } | { error: string } {
  const problem = readError(source, path);
  if (problem !== null) return { error: problem };
  const text = source.readText(path);
  return { text: text ?? "", copied: { source: path, binary: text === null } };
}

/** The error for a vendored file that Belay cannot read. */
export function unreadable(source: Source, item: Construct, path: string, problem: string, site: Site) {
  return error(
    "missing-vendored-files",
    `\`${item.name}\` names \`${shown(source, path)}\`, which cannot be read: ${problem}`,
    site,
  );
}

/** Every file under `folder`, sorted by path. Belay refuses symbolic links,
 * because their targets can be anywhere. */
export function readFolder(source: Source, item: Construct, folder: string, draft: Draft): Entry[] | null {
  const key = vendoredProperty(item.kind) ?? "files";
  const site = source.propertySite(item.anchor, key);
  if (!source.isDirectory(folder)) {
    draft.diagnostics.push(
      error("missing-vendored-files", `\`${item.name}\` names \`${shown(source, folder)}\`, which is not a directory`, site),
    );
    return null;
  }
  let listing;
  try {
    listing = source.ctx.listFiles(folder);
  } catch (thrown) {
    const message = thrown instanceof Error ? thrown.message : String(thrown);
    const problem = message.includes("`: ") ? message.slice(message.indexOf("`: ") + 3) : message;
    draft.diagnostics.push(unreadable(source, item, folder, problem, site));
    return null;
  }
  const found: Entry[] = [];
  let ok = true;
  for (const listed of listing) {
    const path = join(folder, listed.path);
    if (listed.symlink) {
      draft.diagnostics.push(
        error(
          "unsupported-vendored-file",
          `\`${shown(source, path)}\` is a symbolic link; \`${item.name}\` can only bring regular files`,
          site,
        ),
      );
      ok = false;
      continue;
    }
    const read = readFile(source, path);
    if ("error" in read) {
      draft.diagnostics.push(unreadable(source, item, path, read.error, site));
      ok = false;
      continue;
    }
    found.push({ relative: listed.path, text: read.text, copied: read.copied });
  }
  found.sort((a, b) => compareText(a.relative, b.relative));
  return ok ? found : null;
}

/** A construct that a vendored file defines cannot also set other
 * properties: Belay would ignore them without a word. */
export function reportConflicts(source: Source, item: Construct, draft: Draft): void {
  const key = vendoredProperty(item.kind) ?? "files";
  for (const [name, value] of item.properties) {
    if (name === key || value.kind === "null") continue;
    draft.diagnostics.push(
      error(
        "vendored-conflict",
        `\`${item.name}.${name}\` would be ignored: the ${item.kind} is defined by the file \`${key}\` names`,
        source.propertySite(item.anchor, name),
        { help: `remove \`${name}\`, or edit it in the vendored file` },
      ),
    );
  }
}

/** Checks that a vendored definition has the identity that Belay gives it. */
export function checkIdentity(
  source: Source,
  item: Construct,
  path: string,
  found: string | null,
  expected: string,
  draft: Draft,
): void {
  if (found === expected) return;
  const key = vendoredProperty(item.kind) ?? "files";
  const message =
    found !== null
      ? `\`${shown(source, path)}\` is named \`${found}\`, but \`${item.name}\` is emitted as \`${expected}\``
      : `\`${shown(source, path)}\` has no \`name\`, and the ${item.kind} \`${item.name}\` is emitted as \`${expected}\``;
  draft.diagnostics.push(
    error("vendored-name-mismatch", message, source.propertySite(item.anchor, key), {
      help: `set \`name: ${expected}\` in the vendored file, or rename the anchor to match it`,
    }),
  );
}
