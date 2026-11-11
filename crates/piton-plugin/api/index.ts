// @piton/plugin. This module tells what the code of a plugin can do.
//
// The Piton compiler contains this module. Only a plugin can import it. All
// of this module is a thin, typed layer over one host function. That function
// takes the plugin that calls, an operation, and the arguments as JSON. Some
// operations wait, for example a read of the output of a process. They return
// promises. The host resolves these promises from its event loop. Thus a
// plugin can run many processes at the same time with plain async code.
//
// This module takes the host function before the code of a plugin runs. Each
// plugin can get the function only through its own context. The context
// names the plugin. Thus the host checks each operation against the
// capabilities of the plugin that made it.

type Raw = (plugin: string, operation: string, payload: string) => unknown;
let raw: Raw | null = null;

function call<T>(plugin: string, operation: string, payload: unknown = null): T {
  const result = raw!(plugin, operation, JSON.stringify(payload));
  return (typeof result === "string" ? JSON.parse(result) : null) as T;
}

async function callAsync<T>(plugin: string, operation: string, payload: unknown = null): Promise<T> {
  const result = await (raw!(plugin, operation, JSON.stringify(payload)) as Promise<string>);
  return JSON.parse(result) as T;
}

/** A Piton value, as plugin code sees it. Strings are plain text. An anchor
 * as a value is `{ $anchor: name }`. A reference is `{ $reference: name }`. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

/** A location in a source file. A diagnostic uses it. */
export interface Location {
  file: string;
  start: number;
  end: number;
}

/** The project the command runs in. Paths are absolute. */
export interface Project {
  root: string;
  sourceRoot: string;
  entry: string;
  outputDir: string;
  configPath: string | null;
}

/** An anchor from a compilation, or the settings of a project for a plugin. */
export interface Anchor {
  /** Its id in its source compilation, for link spaces. */
  id: number;
  name: string;
  file: string;
  exported: boolean;
  abstract: boolean;
  keyword: string;
  /** Its position in the declarations of its file. It keeps their order. */
  item: number;
  properties: Record<string, Json>;
  /** The location where the anchor names a property itself. If there is no
   * such location, the location of its name. */
  location(property?: string): Location;
}

/** The settings of a project for a plugin. It is an anchor, with the
 * properties of each anchor that it names. An example is the adapters that a
 * belay-config lists. */
export interface Settings extends Anchor {
  anchors: Record<string, Record<string, Json>>;
  /** The declaration of each anchor that it names. Each entry is a file and
   * a name. */
  anchorLocations: Record<string, Location>;
}

/** A Piton value with no flattened parts. A plugin uses it when it must see
 * differences that Json merges. Json does not show text with references as
 * different from plain text. It does not show a dictionary as different from
 * a block with keys and other items. It does not keep all keys in the written
 * order. Here, text holds its plain text, with each reference as the name of
 * its target. It also lists each of its references. */
export type Value =
  | { kind: "null" }
  | { kind: "boolean"; value: boolean }
  | { kind: "number"; value: number }
  | { kind: "text"; text: string; references: { id: number; path: string[] }[] }
  | { kind: "list"; items: Value[] }
  | { kind: "dictionary"; entries: [string, Value][] }
  | { kind: "mixed"; items: (Value | { kind: "entry"; key: string; value: Value })[] }
  | { kind: "anchor"; id: number; name: string }
  | { kind: "reference"; id: number; path: string[]; name: string };

/** The source of the value of a property of an anchor. The source is the
 * anchor whose declaration writes the value. It is the anchor itself or an
 * anchor that it inherits from. */
export interface Slot {
  /** The id of the anchor that writes the value. */
  owner: number;
  /** False when the declaration gives only a type and no value. */
  hasValue: boolean;
  /** The location where the owner names the property. */
  location: Location;
}

/** A file or directory, as a plugin that reads files sees it. */
export interface FileStatus {
  kind: "file" | "directory" | "other";
  /** Its size in bytes. */
  size: number;
  executable: boolean;
}

/** The anchors to read. */
export interface AnchorQuery {
  /** Only anchors with this anchor in their inheritance chain. This anchor
   * itself does not count. `module` is a package, for example `@piton/test`,
   * or the name of an installed package. */
  extends?: { module: string; name: string };
  /** Only anchors declared with `export`. */
  exported?: boolean;
  /** Only anchors that are not abstract. */
  concrete?: boolean;
  /** Only anchors that the source of the project declares. Anchors in
   * tethers/ or in a bundled package do not count. */
  withinSourceRoot?: boolean;
}

interface AnchorRecord {
  id: number;
  name: string;
  file: string;
  exported: boolean;
  abstract: boolean;
  keyword: string;
  item: number;
  properties: Record<string, Json>;
}

/** The location of each anchor in the documents of a plugin. Paths are
 * relative to the project root. */
export interface Layout {
  /** Each document and its anchors, in order. When a property in omitNull
   * is null, the document of that anchor omits the property. */
  documents: { path: string; anchors: { id: number; omitNull?: string[] }[] }[];
  /** Anchors that each go to a separate file with no headings. An example is
   * the SKILL.md of a skill. A reference to such an anchor links to the
   * file. */
  files: { id: number; path: string }[];
}

/** Rendered documents, with links for the references between them. Each
 * method takes the path of the output file. Thus links are relative to that
 * file. */
export class LinkSpace {
  constructor(
    private readonly plugin: string,
    private readonly handle: number,
  ) {}

  /** An anchor as a document. First comes its title, then its properties as
   * sections. */
  document(from: string, id: number, omitNull: string[] = []): string {
    return call<string>(this.plugin, "spaceDocument", { space: this.handle, from, id, omitNull });
  }

  /** Some properties of an anchor as sections, in the order of the anchor.
   * The first level is `level`. It is empty when the anchor has none of the
   * properties. */
  sections(from: string, id: number, keys: string[], level: number): string {
    return call<string>(this.plugin, "spaceSections", { space: this.handle, from, id, keys, level });
  }

  /** A text property as Markdown, with references as links. It is null when
   * the property is not text. */
  text(from: string, id: number, key: string): string | null {
    return call<string | null>(this.plugin, "spaceText", { space: this.handle, from, id, key });
  }

  /** The references to anchors that no file holds. Also each link that the
   * render wrote until now, with the directory of that link. */
  report(): {
    misses: { id: number; anchor: string; path: string[] }[];
    links: { from: string; link: string }[];
  } {
    return call(this.plugin, "spaceReport", { space: this.handle });
  }
}

/** A compiled project. */
export class Compilation {
  constructor(
    private readonly plugin: string,
    private readonly handle: number,
    /** The number of errors that the compilation found. */
    readonly errors: number,
    /** The number of warnings that the compilation found. */
    readonly warnings: number,
  ) {}

  /** The anchors that agree with a query, in the order of file, then name. */
  anchors(query: AnchorQuery = {}): Anchor[] {
    const { plugin, handle } = this;
    return call<AnchorRecord[]>(plugin, "anchors", { handle, query }).map((record) => ({
      ...record,
      location: (property?: string) =>
        call<Location>(plugin, "location", { handle, id: record.id, property: property ?? null }),
    }));
  }

  /** The ids of the anchors that the entry exports, in the order of export.
   * The list includes the anchors that it forwards from other modules. */
  exports(): number[] {
    return call<number[]>(this.plugin, "exports", { handle: this.handle });
  }

  /** The resolved properties of an anchor, in order, with no flattened
   * parts. */
  values(id: number): [string, Value][] {
    return call<[string, Value][]>(this.plugin, "values", { handle: this.handle, id });
  }

  /** The source of the value of a property of an anchor. It is null when the
   * anchor does not have the property. */
  slot(id: number, property: string): Slot | null {
    return call<Slot | null>(this.plugin, "slot", { handle: this.handle, id, property });
  }

  /** Prints the diagnostics of the compilation as `piton check` does. The
   * count comes last. Returns true if one or more were errors. */
  reportDiagnostics(): boolean {
    return call<boolean>(this.plugin, "diagnostics", { handle: this.handle });
  }

  /** Prints the slice of `target`, as `piton slice` does. For each anchor,
   * it cites the location from `layout`, not the source. Returns the exit
   * code. The compilation must be the compilation of the project, as
   * `compile()` makes it. */
  slice(target: string, layout: Layout): number {
    return call<number>(this.plugin, "slice", { handle: this.handle, target, layout });
  }

  /** Makes the layout of documents. The render adds links for the references
   * between them. */
  linkSpace(layout: Layout): LinkSpace {
    return new LinkSpace(this.plugin, call<number>(this.plugin, "space", { handle: this.handle, ...layout }));
  }
}

/** One line that a process wrote. */
export interface Line {
  stream: "stdout" | "stderr";
  line: string;
}

/** A process that runs. The plugin reads its output one line at a time, as
 * the process writes it. */
export class Process {
  constructor(
    private readonly plugin: string,
    private readonly id: number,
  ) {}

  /** The next line that it wrote. It is null after the process exits and the
   * plugin read all lines. */
  next(): Promise<Line | null> {
    return callAsync<Line | null>(this.plugin, "next", { id: this.id });
  }

  async *[Symbol.asyncIterator](): AsyncIterator<Line> {
    for (;;) {
      const line = await this.next();
      if (line === null) return;
      yield line;
    }
  }

  /** Its exit code after it exits, or null when a signal stopped it. */
  wait(): Promise<number | null> {
    return callAsync<number | null>(this.plugin, "wait", { id: this.id });
  }
}

/** The styles for text. On a terminal they are colours. In a pipe, or when
 * the environment sets NO_COLOR, the text is plain. */
export type Style = "error" | "warning" | "success" | "help" | "note" | "name" | "heading" | "dim";

/** A problem to report. When it has a location, the host shows the source at
 * that location, as for a compiler diagnostic. */
export interface Problem {
  code?: string;
  location?: Location;
  help?: string;
}

export interface Terminal {
  /** True for each of stdin, stdout and stderr that is a terminal. */
  stdin: boolean;
  stdout: boolean;
  stderr: boolean;
  /** The width of the terminal in columns, when stdout or stderr is a
   * terminal. */
  width: number | null;
  /** True if the output shows colour. It is false when the environment sets
   * NO_COLOR. */
  color: boolean;
}

/** A file below a directory that the plugin listed. */
export interface ListedFile {
  /** Relative to the listed directory, with forward slashes. */
  path: string;
  executable: boolean;
  /** True for a symbolic link. The host never follows it. */
  symlink: boolean;
}

/** Fields for metadata: text, booleans, numbers, lists of text and nested
 * dictionaries. */
export type Fields = { [key: string]: string | boolean | number | string[] | Fields };

/** A file that a build writes. */
export interface PlannedFile {
  /** Relative to the project root. */
  path: string;
  contents: string;
  /** The source of the file, for diagnostics. */
  origin?: string;
  /** A short description for `piton build --dry-run`. */
  label?: string;
  /** The preview of the editor uses this name to select outputs. An example
   * is a Belay target id. */
  group?: string;
}

/** A file that a build copies with no change. The copy keeps its execute
 * permission. */
export interface PlannedCopy {
  /** The source file, relative to the project root. */
  from: string;
  /** The destination, relative to the project root. */
  to: string;
  origin?: string;
  label?: string;
  group?: string;
}

/** A problem that a build found. */
export interface PlannedDiagnostic {
  severity: "error" | "warning";
  code: string;
  message: string;
  location?: Location;
  help?: string;
}

/** The files that a build wants to write. */
export interface Plan {
  files?: PlannedFile[];
  copies?: PlannedCopy[];
  diagnostics?: PlannedDiagnostic[];
  /** Labels that the manifest records, for example the documentation date of
   * each target. */
  targets?: Record<string, string>;
  /** The location of each anchor, by name. A location is the file and the
   * heading that a reference to the anchor goes to. */
  locations?: Record<string, string>;
}

/** A file that a finding changes, and its full new contents. */
export interface Edit {
  /** Relative to the project root. */
  path: string;
  contents: string;
}

/** A problem in a project that is out of date, and the edits that fix it. */
export interface Finding {
  message: string;
  edits: Edit[];
}

/** The findings of an inspect hook. */
export interface Inspection {
  findings: Finding[];
}

/** An anchor that a reach hook names. It holds the file that declares the
 * anchor, and the name of the anchor. The file is as the `file` of an anchor
 * gives it, or relative to the project root. */
export interface ReachedAnchor {
  file: string;
  name: string;
}

/** The result of a reach hook. It gives the anchors that the plugin reaches
 * itself. */
export interface Reach {
  anchors: ReachedAnchor[];
}

/** All that a plugin can do. The host checks each method that gets access
 * to the machine against the capabilities that the plugin declares. */
export interface Context {
  /** The name of the plugin. */
  readonly plugin: string;
  readonly terminal: Terminal;
  project(): Project;
  /** The settings of the project for this plugin, if there are settings.
   * They are the anchor under `plugins` in piton.config.pi that implements
   * the settings anchor of the plugin. */
  settings(): Settings | null;
  /** Compiles the project. With `workspace`, it compiles each source below
   * the root, not only the sources that the entry reaches. */
  compile(options?: { workspace?: boolean }): Compilation;
  /** The exports of a plugin that this plugin requires. */
  require(name: string): Record<string, unknown>;
  /** The marker for each placeholder that this plugin declares. The
   * placeholder evaluates to the marker. */
  placeholders(): Record<string, string>;
  /** A path from the file `from` to `to`, both relative to the project root. */
  relativePath(from: string, to: string): string;
  /** Encodes with the encoders of the compiler. */
  encode(format: "frontmatter" | "yaml" | "toml", fields: Fields): string;
  encode(format: "tomlString" | "yamlScalar" | "jsonString", text: string): string;
  /** Starts a program that the plugin declares under capabilities.process.
   * The program runs in the project root, or in `cwd` if given. Its stdin is
   * empty. */
  spawn(program: string, args: string[], options?: { cwd?: string; env?: Record<string, string> }): Process;
  /** Runs a program that the plugin declares under capabilities.process. The
   * program has the terminal. The user sees its output and prompts and gives
   * its input. Returns its exit code after it ends, or null when a signal
   * stopped it. */
  exec(program: string, args: string[], options?: { cwd?: string; env?: Record<string, string> }): number | null;
  /** Writes edits and returns the changed paths. Use it in a command that the
   * user ran to make the edits. Each path must agree with
   * capabilities.edits. */
  applyEdits(edits: Edit[]): string[];
  /** Asks the user to choose one of `options` on the terminal. Returns the
   * chosen index. It returns null when the user cancels. It also returns null
   * when no person can answer, because stdin or stdout is not a terminal.
   * Needs capabilities.interactive. */
  choose(message: string, options: string[]): number | null;
  /** Asks the user to type an answer on the terminal. Returns the answer, or
   * null when the user cancels or no terminal is available. Needs
   * capabilities.interactive. */
  ask(message: string, options?: { help?: string }): string | null;
  /** Asks the user to select items of `options` on the terminal. At the
   * start, `defaults` gives the selected items. The user must select
   * `minimum` items or more. Returns the selected indexes, or null when the
   * user cancels or no terminal is available. Needs
   * capabilities.interactive. */
  chooseMany(message: string, options: string[], settings?: { defaults?: number[]; minimum?: number }): number[] | null;
  /** The text of a file in the directory of the plugin, for example a prompt
   * of the plugin. Needs no capability, because the plugin owns the files. */
  resource(path: string): string;
  /** Returns each file below a directory of the plugin, at all depths, in
   * sorted order. Each path is relative to the directory and uses forward
   * slashes. Needs no capability. */
  resources(path: string): string[];
  /** Piton source in the canonical format, as `piton format` writes it. */
  format(source: string, path?: string): string;
  /** Starts a new project. Writes each file into `directory` and prints
   * each written path. Each file is a path relative to `directory` and its
   * contents. `directory` is relative to the directory where piton started.
   * It never overwrites a file. If a file exists, it writes nothing and
   * returns the files that exist. Needs capabilities.scaffold. */
  scaffold(directory: string, files: Edit[]): { existing: string[] };
  /** Tethers the dependencies of the new project in `directory`, as
   * `piton tether` without a source does. Returns `none` when the project
   * has no dependencies, else `installed` or `failed`. With `check`, it
   * tethers nothing and returns `pending` when dependencies exist. Needs
   * capabilities.scaffold. */
  tether(directory: string, options?: { check?: boolean }): "none" | "pending" | "installed" | "failed";
  /** True if PATH contains a program that the plugin declares. */
  onPath(program: string): boolean;
  /** Reads a file in the project. Needs capabilities.files: read. */
  readFile(path: string): string;
  /** All files below a directory in the project, at all depths. Needs
   * capabilities.files: read. */
  listFiles(path: string): ListedFile[];
  /** The file or directory at a path in the project. It follows symbolic
   * links. It is null when the path does not exist. Needs
   * capabilities.files: read. */
  stat(path: string): FileStatus | null;
  /** Writes a temporary file and returns its path. The host removes the file
   * when the command ends. Needs capabilities.temporaryFiles. */
  temporaryFile(contents: string, extension?: string): string;
  /** Resolves after `ms` milliseconds. */
  sleep(ms: number): Promise<void>;
  /** Gives text in a style. */
  paint(style: Style, text: string): string;
  /** Writes to stdout or stderr. The host removes colour where the output
   * cannot show colour. */
  stdout(text: string): void;
  stderr(text: string): void;
  /** Writes to stderr with no change, also terminal control codes. Use it to
   * draw a status line again. It is useful only when terminal.stderr is
   * true. */
  stderrRaw(text: string): void;
  /** The host keeps problems and prints them after all other output. The
   * count comes last. done and stopped print the result line of the
   * command. */
  report: {
    error(message: string, problem?: Problem): void;
    warning(message: string, problem?: Problem): void;
    help(message: string): void;
    note(message: string): void;
    done(message: string): void;
    stopped(message: string): void;
  };
}

/** Runs one command. Arguments come by name. A flag is a boolean. A multiple
 * argument is a list. An argument that the user does not give is null. The
 * returned number is the exit code. When the handler returns nothing, the
 * exit code is 0. */
export type CommandHandler = (
  ctx: Context,
  args: Record<string, Json>,
) => number | void | Promise<number | void>;

export interface Plugin {
  /** A handler for each command that the declaration of the plugin lists. */
  commands?: Record<string, CommandHandler>;
  /** The plan of the files that `piton build` writes. It is for a plugin
   * whose declaration says `build: true`. */
  build?: (ctx: Context) => Plan | Promise<Plan>;
  /** Examines the project before a command runs. It gives the parts that are
   * out of date. It is for a plugin whose declaration says `inspect: true`.
   * The host shows the findings and asks before it applies an edit. */
  inspect?: (ctx: Context) => Inspection | Promise<Inspection>;
  /** Names the anchors that the plugin reaches itself. It is for a plugin
   * whose declaration says `reach: true`. The plugin uses these anchors, but
   * nothing in the specbase goes to them. An example is the tests that the
   * test plugin runs. `piton reach` and the editor count them as roots,
   * together with the entry. Thus they are not unreachable and not
   * unused. */
  reach?: (ctx: Context) => Reach | Promise<Reach>;
  /** The exports for plugins that require this plugin. Each export is a
   * factory. It gets the context of this plugin. Thus it runs with the
   * capabilities of this plugin. */
  exports?: Record<string, (ctx: Context) => unknown>;
}

/** The default export of the entry module of a plugin. */
export function definePlugin(plugin: Plugin): Plugin {
  return plugin;
}

function makeContext(plugin: string, plugins: Map<string, Plugin>, contexts: Map<string, Context>): Context {
  const own = call<{ name: string; requires: string[] }>(plugin, "plugin");
  const exported = new Map<string, Record<string, unknown>>();
  const context: Context = {
    plugin,
    terminal: call<Terminal>(plugin, "terminal"),
    project: () => call<Project>(plugin, "project"),
    settings() {
      const record = call<
        | (AnchorRecord & {
            spans: Record<string, Location>;
            anchors: Record<string, Record<string, Json>>;
            anchorLocations: Record<string, Location>;
          })
        | null
      >(plugin, "settings");
      if (record === null) return null;
      const { spans, ...rest } = record;
      return {
        ...rest,
        location: (property?: string) => (property !== undefined && spans[property]) || spans[""],
      };
    },
    compile(options = {}) {
      const compiled = call<{ handle: number; errors: number; warnings: number }>(plugin, "compile", {
        workspace: options.workspace ?? false,
      });
      return new Compilation(plugin, compiled.handle, compiled.errors, compiled.warnings);
    },
    require(name) {
      if (!own.requires.includes(name)) {
        throw new Error(`plugin \`${plugin}\` requires \`${name}\` without listing it under requires`);
      }
      let value = exported.get(name);
      if (value === undefined) {
        const factories = plugins.get(name)?.exports ?? {};
        const target = contexts.get(name)!;
        value = Object.fromEntries(Object.entries(factories).map(([key, factory]) => [key, factory(target)]));
        exported.set(name, value);
      }
      return value;
    },
    placeholders: () => call<Record<string, string>>(plugin, "placeholders"),
    relativePath: (from, to) => call<string>(plugin, "relativePath", { from, to }),
    encode: (format: string, value: unknown) => call<string>(plugin, "encode", { format, value }),
    spawn: (program, args, options = {}) =>
      new Process(plugin, call<number>(plugin, "spawn", { program, args, cwd: options.cwd ?? null, env: options.env ?? {} })),
    exec: (program, args, options = {}) =>
      call<number | null>(plugin, "exec", { program, args, cwd: options.cwd ?? null, env: options.env ?? {} }),
    resource: (path) => call<string>(plugin, "resource", { path }),
    resources: (path) => call<string[]>(plugin, "resources", { path }),
    ask: (message, options = {}) => call<string | null>(plugin, "ask", { message, help: options.help ?? null }),
    chooseMany: (message, options, settings = {}) =>
      call<number[] | null>(plugin, "chooseMany", {
        message,
        options,
        defaults: settings.defaults ?? [],
        minimum: settings.minimum ?? 0,
      }),
    format: (source, path = "index.pi") => call<string>(plugin, "format", { source, path }),
    scaffold: (directory, files) => call<{ existing: string[] }>(plugin, "scaffold", { directory, files }),
    tether: (directory, options = {}) =>
      call<"none" | "pending" | "installed" | "failed">(plugin, "tether", { directory, check: options.check ?? false }),
    choose: (message, options) => call<number | null>(plugin, "choose", { message, options }),
    applyEdits: (edits) => call<string[]>(plugin, "applyEdits", { edits }),
    onPath: (program) => call<boolean>(plugin, "onPath", { program }),
    readFile: (path) => call<string>(plugin, "readFile", { path }),
    listFiles: (path) => call<ListedFile[]>(plugin, "listFiles", { path }),
    stat: (path) => call<FileStatus | null>(plugin, "stat", { path }),
    temporaryFile: (contents, extension = "") => call<string>(plugin, "temporaryFile", { contents, extension }),
    sleep: (ms) => callAsync<null>(plugin, "sleep", { ms }).then(() => undefined),
    paint: (style, text) => call<string>(plugin, "paint", { style, text }),
    stdout: (text) => void call(plugin, "write", { stream: "stdout", text }),
    stderr: (text) => void call(plugin, "write", { stream: "stderr", text }),
    stderrRaw: (text) => void call(plugin, "write", { stream: "stderrRaw", text }),
    report: {
      error: (message, problem = {}) => void call(plugin, "report", { kind: "error", message, ...problem }),
      warning: (message, problem = {}) => void call(plugin, "report", { kind: "warning", message, ...problem }),
      help: (message) => void call(plugin, "report", { kind: "help", message }),
      note: (message) => void call(plugin, "report", { kind: "note", message }),
      done: (message) => void call(plugin, "report", { kind: "done", message }),
      stopped: (message) => void call(plugin, "report", { kind: "stopped", message }),
    },
  };
  return context;
}

/** Takes the host function one time, before the import of a plugin. It is
 * not part of the API for plugins. A plugin that calls it gets an error,
 * because this module already has the host function. */
export function __setup() {
  const global = globalThis as unknown as { __piton?: Raw };
  if (raw !== null || typeof global.__piton !== "function") {
    throw new Error("the host is already set up");
  }
  raw = global.__piton;
  delete global.__piton;
  const plugins = new Map<string, Plugin>();
  const contexts = new Map<string, Context>();
  const context = (name: string) => {
    let found = contexts.get(name);
    if (found === undefined) {
      found = makeContext(name, plugins, contexts);
      contexts.set(name, found);
    }
    return found;
  };
  return {
    register(name: string, plugin: Plugin) {
      plugins.set(name, plugin ?? {});
      context(name);
    },
    async command(name: string, command: string, args: string): Promise<number> {
      const handler = plugins.get(name)?.commands?.[command];
      if (typeof handler !== "function") {
        throw new Error(`the plugin has no handler for its command \`${command}\``);
      }
      const code = await handler(context(name), JSON.parse(args));
      return typeof code === "number" ? code : 0;
    },
    async inspect(name: string): Promise<Inspection> {
      const inspect = plugins.get(name)?.inspect;
      if (typeof inspect !== "function") {
        throw new Error("the plugin's declaration says inspect: true, but it has no inspect");
      }
      return (await inspect(context(name))) ?? { findings: [] };
    },
    async reach(name: string): Promise<Reach> {
      const reach = plugins.get(name)?.reach;
      if (typeof reach !== "function") {
        throw new Error("the plugin's declaration says reach: true, but it has no reach");
      }
      return (await reach(context(name))) ?? { anchors: [] };
    },
    async build(name: string): Promise<Plan> {
      const build = plugins.get(name)?.build;
      if (typeof build !== "function") {
        throw new Error("the plugin's declaration says build: true, but it has no build");
      }
      return (await build(context(name))) ?? {};
    },
  };
}
