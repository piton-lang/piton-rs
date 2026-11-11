// This module holds the plan while Belay makes it.

import type { Diagnostic } from "./diagnostics";
import { clean } from "./paths";

/** What made an output file. Diagnostics name it. */
export type OutputKind =
  | "instruction"
  | "skill"
  /** A file that a skill brings from its vendored folder, next to its SKILL.md. */
  | "skill file"
  | "command"
  | "command policy"
  | "agent"
  | "reference"
  | "shape reference";

/** A file that Belay copies as it is from the vendored files of a construct. */
export interface Copied {
  /** The absolute path of the source file. */
  source: string;
  /** True if the file is not UTF-8 text. Then `contents` is empty. */
  binary: boolean;
}

/** One planned file, with the anchors whose content it holds, in order. */
export interface Planned {
  /** Relative to the project root. */
  path: string;
  contents: string;
  kind: OutputKind;
  target: string;
  /** The anchor or anchors that the file comes from, for diagnostics. */
  origin: string;
  /** Set when the file is a copy. Belay rewrites nothing in a copy. */
  copied: Copied | null;
  anchors: number[];
}

/** The bytes of a planned file, as a key that tells two files apart. */
export function data(file: Planned): string {
  return file.copied !== null && file.copied.binary ? `\u0000${file.copied.source}` : file.contents;
}

/** The plan while Belay makes it. */
export class Draft {
  files: Planned[] = [];
  diagnostics: Diagnostic[] = [];
  /** The links that rendering wrote, each with the directory that it is
   * relative to. The checks look only at these links. A link that an author
   * wrote, to an image or to a page of their own, belongs to the author. */
  private readonly links = new Set<string>();

  push(file: Planned): void {
    this.files.push(file);
  }

  addLink(from: string, link: string): void {
    this.links.add(`${clean(from)}\u0000${link}`);
  }

  wroteLink(from: string, link: string): boolean {
    return this.links.has(`${clean(from)}\u0000${link}`);
  }
}
