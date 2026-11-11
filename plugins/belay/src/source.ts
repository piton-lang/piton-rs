// This module gives Belay what it reads from the host.
//
// Belay reads the compiled project, the project configuration and some
// files. The class `Source` holds the context and the compilation. It keeps
// what it read, so Belay asks the host for each fact one time only.

import type { Anchor, Compilation, Context, FileStatus, Location, Project, Value } from "@piton/plugin";
import { isAbsolute, join, normalize, parent, startsWith } from "./paths";

/** The package that declares the constructs. */
export const PACKAGE = "@piton/belay";

/** A place in a source file, for a diagnostic. */
export type Site = Location;

/** The project, with the roots that the Belay settings give. */
export interface BelayProject extends Project {
  /** The code root: `codeRoot` of the settings, or `src` in the project. */
  codeRoot: string;
  /** The shape root: `shapeRoot` of the settings, or null. */
  shapeRoot: string | null;
}

/** What other plugins use: the code root and the shape root of the Belay
 * settings, or null when the project does not configure Belay. */
export function rootsExport(ctx: Context): { codeRoot: string | null; shapeRoot: string | null } {
  if (ctx.settings() === null) return { codeRoot: null, shapeRoot: null };
  const { codeRoot, shapeRoot } = roots(ctx, ctx.project());
  return { codeRoot, shapeRoot };
}

/** Reads the roots from the Belay settings. Each root is relative to the
 * configuration file, like the other paths in the configuration. */
function roots(ctx: Context, project: Project): BelayProject {
  const settings = ctx.settings();
  const base = project.configPath === null ? project.root : (parent(project.configPath) ?? project.root);
  const path = (key: string): string | null => {
    const value = settings?.properties[key];
    return typeof value === "string" && value.trim().length > 0 ? normalize(join(base, value.trim())) : null;
  };
  return { ...project, codeRoot: path("codeRoot") ?? join(base, "src"), shapeRoot: path("shapeRoot") };
}

export class Source {
  readonly project: BelayProject;
  readonly compilation: Compilation;
  private readonly records = new Map<number, Anchor>();
  private readonly valueCache = new Map<number, [string, Value][]>();
  private readonly siteCache = new Map<string, Site>();
  private readonly statCache = new Map<string, FileStatus | null>();

  constructor(readonly ctx: Context) {
    this.project = roots(ctx, ctx.project());
    this.compilation = ctx.compile();
    for (const record of this.compilation.anchors()) this.records.set(record.id, record);
  }

  /** The anchor with this id. */
  anchor(id: number): Anchor {
    const record = this.records.get(id);
    if (record === undefined) throw new Error(`the compilation has no anchor with the id ${id}`);
    return record;
  }

  /** The name of the anchor with this id. */
  name(id: number): string {
    return this.anchor(id).name;
  }

  /** The module path of the anchor with this id. */
  file(id: number): string {
    return this.anchor(id).file;
  }

  /** The ids of the anchors that extend an anchor of Belay's package. */
  extending(name: string): number[] {
    return this.compilation.anchors({ extends: { module: PACKAGE, name } }).map((record) => record.id);
  }

  /** The properties of an anchor, with nothing flattened. */
  values(id: number): [string, Value][] {
    let values = this.valueCache.get(id);
    if (values === undefined) {
      values = this.compilation.values(id);
      this.valueCache.set(id, values);
    }
    return values;
  }

  /** Where an anchor is declared: its file and its name. */
  anchorSite(id: number): Site {
    const key = `${id}`;
    let site = this.siteCache.get(key);
    if (site === undefined) {
      site = this.anchor(id).location();
      this.siteCache.set(key, site);
    }
    return site;
  }

  /** Where the declaration that gives a property its value names it, or
   * where the anchor is declared. */
  propertySite(id: number, key: string): Site {
    const slot = this.compilation.slot(id, key);
    if (slot !== null && (slot.location.start !== 0 || slot.location.end !== 0)) return slot.location;
    return this.anchorSite(id);
  }

  /** The module of the declaration that gives a property its value. */
  declaringModule(id: number, key: string): string {
    const slot = this.compilation.slot(id, key);
    return this.file(slot === null ? id : slot.owner);
  }

  /** What is at a path, or null. The path is absolute, or relative to the
   * project root. */
  stat(path: string): FileStatus | null {
    const full = isAbsolute(path) ? path : join(this.project.root, path);
    if (this.statCache.has(full)) return this.statCache.get(full)!;
    let status: FileStatus | null = null;
    if (startsWith(full, this.project.root)) {
      try {
        status = this.ctx.stat(full);
      } catch {
        status = null;
      }
    }
    this.statCache.set(full, status);
    return status;
  }

  isFile(path: string): boolean {
    return this.stat(path)?.kind === "file";
  }

  isDirectory(path: string): boolean {
    return this.stat(path)?.kind === "directory";
  }

  /** The text of a file, or null when it cannot be read as text. */
  readText(path: string): string | null {
    try {
      return this.ctx.readFile(path);
    } catch {
      return null;
    }
  }
}
