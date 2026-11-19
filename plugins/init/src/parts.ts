// The parts `piton init` builds a project from, read from the plugin's own
// templates directory.
//
// Every directory in templates/ is a part, like `basic` or `belay`, holding
// files laid out as the project lays them out. A project gets the part of its
// type, then the parts its answers add. Within a part, files under `shape/`
// and `code/` go beneath Belay's shape and code roots, files under
// `.adapters/<target id>/` only when that adapter is chosen, and the
// placeholders below are filled in. When several parts have a file at the
// same path, they are joined into one.

import type { Context } from "@piton/plugin";

/** The directory in the plugin that holds the parts. */
const ROOT = "templates";

/** The directory in a part holding, for each adapter by target id, files
 * written only when that adapter is chosen. */
const ADAPTER_FILES = ".adapters/";

/** The directories in a part whose files go beneath Belay's roots. */
const SHAPE_FILES = "shape/";
const CODE_FILES = "code/";

export interface File {
  /** Relative to the new project. */
  path: string;
  contents: string;
}

/** What fills a part in. */
export interface Filling {
  /** The project's name, for `{{name}}`. */
  name: string;
  /** Target ids of the adapters chosen. */
  adapters: string[];
  /** Belay's roots, relative to the project without `./`, like `src` and
   * `spec/shape`; `.` for the project root. */
  codeRoot: string;
  shapeRoot: string;
  /** The specification root, relative to the project, like `spec`. */
  specRoot: string;
}

/** The files of `parts`, in order, filled in and with files at the same path
 * joined. */
export function files(ctx: Context, parts: string[], filling: Filling): File[] {
  const placed = new Map<string, string[]>();
  for (const part of parts) {
    for (const file of partFiles(ctx, part, filling)) {
      const existing = placed.get(file.path);
      if (existing === undefined) placed.set(file.path, [file.contents]);
      else existing.push(file.contents);
    }
  }
  return [...placed.entries()].map(([path, contents]) => ({
    path,
    contents: contents.length === 1 ? contents[0] : ctx.format(join(contents), path),
  }));
}

/** The files `part` adds, placed and filled in. */
function partFiles(ctx: Context, part: string, filling: Filling): File[] {
  const paths = ctx.resources(`${ROOT}/${part}`);
  if (paths.length === 0) throw new Error(`${ROOT}/${part} has no files`);
  return paths.flatMap((source) => {
    const path = place(source, filling);
    if (path === null) return [];
    return [{ path, contents: fill(ctx.resource(`${ROOT}/${part}/${source}`), filling) }];
  });
}

/** Where a part's file is written in the project, or null when it isn't. */
function place(path: string, filling: Filling): string | null {
  if (path.startsWith(ADAPTER_FILES)) {
    // `.adapters/<target id>/<path>` is written at `<path>`, and only when
    // that adapter is chosen.
    const rest = path.slice(ADAPTER_FILES.length);
    const slash = rest.indexOf("/");
    if (slash < 0 || !filling.adapters.includes(rest.slice(0, slash))) return null;
    return rest.slice(slash + 1);
  }
  if (path.startsWith(SHAPE_FILES)) return beneath(filling.shapeRoot, path.slice(SHAPE_FILES.length));
  if (path.startsWith(CODE_FILES)) {
    // The code root's own files aren't written into the project root.
    if (filling.codeRoot === ".") return null;
    return beneath(filling.codeRoot, path.slice(CODE_FILES.length));
  }
  return path;
}

function beneath(root: string, path: string): string {
  return root === "." ? path : `${root}/${path}`;
}

/** `contents` with the placeholders replaced. */
function fill(contents: string, filling: Filling): string {
  const shapeFromSpec = filling.shapeRoot.slice(filling.specRoot.length + 1);
  const values: Record<string, string> = {
    name: filling.name,
    codeRoot: filling.codeRoot === "." ? "" : `${filling.codeRoot}/`,
    shapeRoot: shapeFromSpec,
    specRoot: shapeFromSpec.split("/").map(() => "..").join("/"),
  };
  return contents.replace(/\{\{(\w+)\}\}/g, (whole, key: string) => values[key] ?? whole);
}

/** The files of several parts at one path as one: what each has before its
 * first declaration, its comments, uses and imports, in order, then the rest
 * of each in order. */
function join(contents: string[]): string {
  const heads: string[] = [];
  const bodies: string[] = [];
  for (const text of contents) {
    const lines = text.replace(/\n+$/, "").split("\n");
    let split = lines.findIndex((line) => !isHeading(line));
    if (split < 0) split = lines.length;
    const head = lines.slice(0, split).join("\n").trim();
    const body = lines.slice(split).join("\n").trim();
    // A head that opens with a comment keeps a blank line above it.
    if (head !== "") heads.push(heads.length > 0 && head.startsWith("//") ? `\n${head}` : head);
    if (body !== "") bodies.push(body);
  }
  return [heads.join("\n"), ...bodies].filter((block) => block !== "").join("\n\n") + "\n";
}

/** Whether a line belongs before a file's first declaration. Declarations
 * start at the margin, so an indented line there continues an import. */
function isHeading(line: string): boolean {
  const trimmed = line.trim();
  return (
    trimmed === "" ||
    line !== line.trimStart() ||
    trimmed.startsWith(")") ||
    trimmed.startsWith("//") ||
    trimmed.startsWith("use ") ||
    trimmed.startsWith("from ")
  );
}
