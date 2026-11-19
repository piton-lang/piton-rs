// This module makes a Piton module from an Emul file.
//
// The plugin does not compile Emul. It reads only the declaration lines at the
// start of a line. Emul writes them in the same form as Piton. For each design
// that the file exports, it writes a concrete emul-design anchor of
// @piton/emul. The anchor holds the name, the kind and the file of the design.
// Exports from other Emul files become usual Piton exports from those files.
// Thus the compiler treats the result as all other modules.
//
// Each anchor also references the designs that the design uses: the
// components that it extends and the designs that it instances. A design that
// uses a local design that the file does not export uses what that design
// uses. Thus `piton reach` goes from a composition to its components.
//
// `emul check` finds the errors of Emul. This module does not report them.

/** The extension of an Emul file. */
export const EXTENSION = "emul";

/** One local design of an Emul file. */
interface Design {
  kind: "component" | "composition";
  name: string;
  exported: boolean;
  /** The names of other designs in the text of the design. */
  names: Set<string>;
}

/** Tells if a path, relative to the project root, names a file. */
export type IsFile = (path: string) => boolean;

/** Returns the Piton source for the Emul file at `file` with the text `text`.
 * `file` is relative to the project root, with forward slashes. */
export function synthesize(text: string, file: string, isFile: IsFile): string {
  const directory = parent(file);
  const designs: Design[] = [];
  // These lines stay in the Piton module: imports and exports from other
  // files.
  const forwarded: string[] = [];
  const exportedNames: string[] = [];
  // The names that the file imports: the name in the file, after `as`.
  const imported = new Set<string>();

  for (const header of headers(text)) {
    const words = header.split(/\s+/);
    if (words[0] === "export" && (words[1] === "component" || words[1] === "composition") && words[2]) {
      designs.push({ kind: words[1], name: identifier(words[2]), exported: true, names: new Set() });
    } else if ((words[0] === "component" || words[0] === "composition") && words[1]) {
      designs.push({ kind: words[0], name: identifier(words[1]), exported: false, names: new Set() });
    } else if (words[0] === "export") {
      for (const name of header.slice("export".length).split(",")) {
        if (name.trim() !== "") exportedNames.push(name.trim());
      }
    } else if (words[0] === "from" && words[1] && (words[2] === "import" || words[2] === "export")) {
      const verb = words[2];
      const at = header.indexOf(` ${verb}`);
      const rest = header.slice(at + verb.length + 1).trim();
      forwarded.push(`from ${emulPath(words[1], directory, isFile)} ${verb} ${rest}`);
      if (verb === "import") {
        for (const item of rest.split(",")) {
          const parts = item.trim().split(/\s+/);
          const local = parts[parts.length - 1];
          if (local && local !== "*") imported.add(local);
        }
      }
    }
  }

  // `export Name` exports a local design, or a name that the file imports.
  const reexports: string[] = [];
  for (const name of exportedNames) {
    const design = designs.find((candidate) => candidate.name === name);
    if (design) design.exported = true;
    else reexports.push(name);
  }

  // The names in the body of each design, then what each design uses, through
  // the local designs that the file does not export.
  const known = new Set<string>([...imported, ...designs.map((design) => design.name)]);
  for (const [name, body] of bodies(text)) {
    const design = designs.find((candidate) => candidate.name === name);
    if (!design) continue;
    for (const word of body.match(/[A-Za-z_][A-Za-z0-9_-]*/g) ?? []) {
      if (word !== name && known.has(word)) design.names.add(word);
    }
  }
  const uses = (design: Design, seen: Set<string>): Set<string> => {
    const out = new Set<string>();
    for (const name of design.names) {
      const local = designs.find((candidate) => candidate.name === name);
      if (!local || local.exported) out.add(name);
      else if (!seen.has(name)) {
        seen.add(name);
        for (const inner of uses(local, seen)) out.add(inner);
      }
    }
    return out;
  };

  let out = `// The Emul plugin made this module from the Emul file \`${file}\`.\nuse @piton/emul\n`;
  for (const line of forwarded) out += `${line}\n`;
  for (const design of designs.filter((candidate) => candidate.exported)) {
    out +=
      `\nexport emul-design ${design.name}:\n` +
      `    name: ${design.name}\n` +
      `    kind: ${design.kind}\n` +
      `    file: ${file}\n`;
    const used = [...uses(design, new Set([design.name]))].filter((name) => name !== design.name).sort();
    if (used.length > 0) {
      out += `    uses:\n`;
      for (const name of used) out += `        - @{${name}}\n`;
    }
  }
  if (reexports.length > 0) out += `\nexport ${reexports.join(", ")}\n`;
  return out;
}

/** The declaration lines of an Emul file, each as one line. An import or an
 * export can continue on indented lines. They join the line above. Each
 * continuation line holds items of the list, and `emul format` writes one name
 * on each line with no comma. Thus a comma goes between the lines. */
function headers(text: string): string[] {
  const out: string[] = [];
  let open = false;
  for (const line of text.split(/\r?\n/)) {
    const indented = line.startsWith(" ") || line.startsWith("\t");
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("//")) continue;
    if (indented) {
      if (open && out.length > 0) {
        const last = out[out.length - 1];
        const listStart = /\s(import|export)$/.test(last) || last.endsWith(",");
        out[out.length - 1] = listStart ? `${last} ${trimmed}` : `${last}, ${trimmed}`;
      }
      continue;
    }
    open =
      trimmed.startsWith("from ") ||
      (trimmed.startsWith("export ") && !trimmed.includes(":") && !trimmed.includes("{"));
    out.push(trimmed);
  }
  return out;
}

/** The body of each design: its declaration line and the lines below it,
 * without comments and text. A body ends at the next line that starts at the
 * left edge, other than the `}` that closes a composition. */
function bodies(text: string): Map<string, string> {
  const out = new Map<string, string>();
  let current: string | null = null;
  for (const line of text.split(/\r?\n/)) {
    const clean = line.replace(/"(?:[^"\\]|\\.)*"/g, " ").replace(/\/\/.*$/, "");
    const atEdge = clean.length > 0 && !/^\s/.test(clean);
    if (atEdge && clean.trim() !== "}") {
      const words = clean.trim().split(/\s+/);
      const at = words[0] === "export" ? 1 : 0;
      current =
        (words[at] === "component" || words[at] === "composition") && words[at + 1]
          ? identifier(words[at + 1])
          : null;
      if (current) {
        // The declaration line holds `extends`; the name itself is not a use.
        out.set(current, words.slice(at + 2).join(" "));
        continue;
      }
    }
    if (current) out.set(current, `${out.get(current) ?? ""}\n${clean}`);
  }
  return out;
}

/** The name of a declaration, without the `:` or `{` after it. */
function identifier(word: string): string {
  return word.replace(/[:{]+$/, "");
}

/** The directory part of a path, or `.` when the path has none. */
function parent(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "." : path.slice(0, at) || "/";
}

/** Joins a directory and a relative path, and removes `.` and `..` parts. */
function join(directory: string, relative: string): string {
  const parts: string[] = [];
  for (const part of `${directory}/${relative}`.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === ".." && parts.length > 0 && parts[parts.length - 1] !== "..") parts.pop();
    else parts.push(part);
  }
  return parts.join("/");
}

/** A module path in an Emul file, as a Piton path that names the Emul file.
 * Emul makes the extension optional. Piton needs it to know that the path
 * names an Emul file. */
function emulPath(written: string, directory: string, isFile: IsFile): string {
  const trimmed = written.replace(/\/+$/, "");
  const last = trimmed.split("/").pop() ?? trimmed;
  if (trimmed === "" || last === "." || last === "..") {
    return `${trimmed === "" ? "." : trimmed}/index.${EXTENSION}`;
  }
  if (trimmed.endsWith(`.${EXTENSION}`)) return trimmed;
  // A relative path can name `Other.emul` or `Other/index.emul`.
  if (!trimmed.startsWith("/")) {
    const file = join(directory, `${trimmed}.${EXTENSION}`);
    const index = join(directory, `${trimmed}/index.${EXTENSION}`);
    if (!isFile(file) && isFile(index)) return `${trimmed}/index.${EXTENSION}`;
  }
  return `${trimmed}.${EXTENSION}`;
}
