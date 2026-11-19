// This module makes a Piton module from an Emul file.
//
// The plugin does not compile Emul. It reads only the declaration lines at the
// start of a line. Emul writes them in the same form as Piton. For each design
// that the file exports, it writes a concrete emul-design anchor of
// @piton/emul. The anchor holds the name, the kind and the file of the design.
// Exports from other Emul files become usual Piton exports from those files.
// Thus the compiler treats the result as all other modules.
//
// `emul check` finds the errors of Emul. This module does not report them.

/** The extension of an Emul file. */
export const EXTENSION = "emul";

/** One local design of an Emul file. */
interface Design {
  kind: "component" | "composition";
  name: string;
  exported: boolean;
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

  for (const header of headers(text)) {
    const words = header.split(/\s+/);
    if (words[0] === "export" && (words[1] === "component" || words[1] === "composition") && words[2]) {
      designs.push({ kind: words[1], name: identifier(words[2]), exported: true });
    } else if ((words[0] === "component" || words[0] === "composition") && words[1]) {
      designs.push({ kind: words[0], name: identifier(words[1]), exported: false });
    } else if (words[0] === "export") {
      for (const name of header.slice("export".length).split(",")) {
        if (name.trim() !== "") exportedNames.push(name.trim());
      }
    } else if (words[0] === "from" && words[1] && (words[2] === "import" || words[2] === "export")) {
      const verb = words[2];
      const at = header.indexOf(` ${verb}`);
      const rest = header.slice(at + verb.length + 1).trim();
      forwarded.push(`from ${emulPath(words[1], directory, isFile)} ${verb} ${rest}`);
    }
  }

  // `export Name` exports a local design, or a name that the file imports.
  const reexports: string[] = [];
  for (const name of exportedNames) {
    const design = designs.find((candidate) => candidate.name === name);
    if (design) design.exported = true;
    else reexports.push(name);
  }

  let out = `// The Emul plugin made this module from the Emul file \`${file}\`.\nuse @piton/emul\n`;
  for (const line of forwarded) out += `${line}\n`;
  for (const design of designs.filter((candidate) => candidate.exported)) {
    out +=
      `\nexport emul-design ${design.name}:\n` +
      `    name: ${design.name}\n` +
      `    kind: ${design.kind}\n` +
      `    file: ${file}\n`;
  }
  if (reexports.length > 0) out += `\nexport ${reexports.join(", ")}\n`;
  return out;
}

/** The declaration lines of an Emul file, each as one line. An import or an
 * export can continue on indented lines. They join the line above. */
function headers(text: string): string[] {
  const out: string[] = [];
  let open = false;
  for (const line of text.split(/\r?\n/)) {
    const indented = line.startsWith(" ") || line.startsWith("\t");
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("//")) continue;
    if (indented) {
      if (open && out.length > 0) out[out.length - 1] += ` ${trimmed}`;
      continue;
    }
    open =
      trimmed.startsWith("from ") ||
      (trimmed.startsWith("export ") && !trimmed.includes(":") && !trimmed.includes("{"));
    out.push(trimmed);
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
