// @piton/plugin: what a plugin's code can do.
//
// Bundled with the Piton compiler and importable only from inside a plugin.
// Everything here is a thin, typed layer over one host function, which takes
// the plugin making the call, an operation, and its arguments as JSON.
// Operations that wait, like reading a process's output, return promises the
// host resolves from its event loop, so a plugin can run many processes at
// once with plain async code.
//
// The host function is taken by this module before any plugin's code runs,
// and each plugin only ever reaches it through its own context, which names
// the plugin. So every operation is checked against the capabilities of the
// plugin that made it.

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

/** A Piton value as plugin code sees it. Strings are plain text; an anchor
 * used as a value is `{ $anchor: name }`, a reference `{ $reference: name }`. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

/** A place in a source file, for pointing a diagnostic at. */
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

/** An anchor read from a compilation, or a project's settings for a plugin. */
export interface Anchor {
  /** Its id in the compilation it came from, for link spaces. */
  id: number;
  name: string;
  file: string;
  exported: boolean;
  abstract: boolean;
  keyword: string;
  /** Its place among its file's declarations, for keeping their order. */
  item: number;
  properties: Record<string, Json>;
  /** Where the anchor names a property itself, else where its name is. */
  location(property?: string): Location;
}

/** A project's settings for a plugin: an anchor, with the properties of
 * every anchor it names, like the adapters a belay-config lists. */
export interface Settings extends Anchor {
  anchors: Record<string, Record<string, Json>>;
  /** Where each anchor it names is declared: its file and its name. */
  anchorLocations: Record<string, Location>;
}

/** A Piton value with nothing flattened, for a plugin that has to tell apart
 * what Json merges: text with references from plain text, a dictionary from a
 * block that mixes keys with other items, and keys in the order written,
 * whatever they look like. Text is its plain text, with references written as
 * the names they point at, and lists each reference it holds. */
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

/** Where an anchor's property gets its value: the anchor whose declaration
 * writes it, which is the anchor itself or one it inherits from. */
export interface Slot {
  /** The id of the anchor that writes the value. */
  owner: number;
  /** False when only a type was declared for it, and no value. */
  hasValue: boolean;
  /** Where the owner names the property. */
  location: Location;
}

/** A file or directory, as a plugin that reads files can see it. */
export interface FileStatus {
  kind: "file" | "directory" | "other";
  /** Its size in bytes. */
  size: number;
  executable: boolean;
}

/** Which anchors to read. */
export interface AnchorQuery {
  /** Only anchors with this one in their inheritance chain, not counting it.
   * `module` is a package like `@piton/test`, or an installed package's name. */
  extends?: { module: string; name: string };
  /** Only anchors declared with `export`. */
  exported?: boolean;
  /** Only anchors that aren't abstract. */
  concrete?: boolean;
  /** Only anchors declared in the project's own source, not in tethers/ or
   * a bundled package. */
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

/** Where a plugin's documents put each anchor. Paths are relative to the
 * project root. */
export interface Layout {
  /** Each document and the anchors it holds, in order. A property listed in
   * omitNull is left out of that anchor's document when it is null. */
  documents: { path: string; anchors: { id: number; omitNull?: string[] }[] }[];
  /** Anchors written to a file of their own, without headings, like a
   * skill's SKILL.md: a reference to one links to the file. */
  files: { id: number; path: string }[];
}

/** Documents rendered with references linked between them. Each method takes
 * the path of the file being written, so links are relative to it. */
export class LinkSpace {
  constructor(
    private readonly plugin: string,
    private readonly handle: number,
  ) {}

  /** An anchor as a document: its title, then its properties as sections. */
  document(from: string, id: number, omitNull: string[] = []): string {
    return call<string>(this.plugin, "spaceDocument", { space: this.handle, from, id, omitNull });
  }

  /** Some of an anchor's properties as sections, in the anchor's order,
   * starting at `level`; empty when it has none of them. */
  sections(from: string, id: number, keys: string[], level: number): string {
    return call<string>(this.plugin, "spaceSections", { space: this.handle, from, id, keys, level });
  }

  /** A text property as Markdown, references as links; null when the
   * property isn't text. */
  text(from: string, id: number, key: string): string | null {
    return call<string | null>(this.plugin, "spaceText", { space: this.handle, from, id, key });
  }

  /** References to anchors nothing wrote, and every link written so far
   * with the directory it is relative to. */
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
    /** How many errors compiling found. */
    readonly errors: number,
    /** How many warnings compiling found. */
    readonly warnings: number,
  ) {}

  /** The anchors matching a query, ordered by file, then name. */
  anchors(query: AnchorQuery = {}): Anchor[] {
    const { plugin, handle } = this;
    return call<AnchorRecord[]>(plugin, "anchors", { handle, query }).map((record) => ({
      ...record,
      location: (property?: string) =>
        call<Location>(plugin, "location", { handle, id: record.id, property: property ?? null }),
    }));
  }

  /** The ids of the anchors the entry exports, in the order it exports them,
   * the ones it forwards from other modules included. */
  exports(): number[] {
    return call<number[]>(this.plugin, "exports", { handle: this.handle });
  }

  /** An anchor's resolved properties, in order, with nothing flattened. */
  values(id: number): [string, Value][] {
    return call<[string, Value][]>(this.plugin, "values", { handle: this.handle, id });
  }

  /** Where an anchor's property gets its value, or null when the anchor has
   * no such property. */
  slot(id: number, property: string): Slot | null {
    return call<Slot | null>(this.plugin, "slot", { handle: this.handle, id, property });
  }

  /** Prints the compilation's diagnostics as `piton check` does, with the
   * count last, and says whether any were errors. */
  reportDiagnostics(): boolean {
    return call<boolean>(this.plugin, "diagnostics", { handle: this.handle });
  }

  /** Prints the slice of `target`, like `piton slice`, citing where `layout`
   * puts each anchor instead of the source, and returns the exit code. The
   * compilation has to be the project's own, as `compile()` makes it. */
  slice(target: string, layout: Layout): number {
    return call<number>(this.plugin, "slice", { handle: this.handle, target, layout });
  }

  /** Lays out documents to render with references linked between them. */
  linkSpace(layout: Layout): LinkSpace {
    return new LinkSpace(this.plugin, call<number>(this.plugin, "space", { handle: this.handle, ...layout }));
  }
}

/** One line a process wrote. */
export interface Line {
  stream: "stdout" | "stderr";
  line: string;
}

/** A running process. Its output is read line by line as it is written. */
export class Process {
  constructor(
    private readonly plugin: string,
    private readonly id: number,
  ) {}

  /** The next line it wrote, or null once it has exited and every line has
   * been read. */
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

  /** Its exit code once it has exited, or null when a signal stopped it. */
  wait(): Promise<number | null> {
    return callAsync<number | null>(this.plugin, "wait", { id: this.id });
  }
}

/** The styles text can be painted in. On a terminal they are colours; piped,
 * and with NO_COLOR set, the text is plain. */
export type Style = "error" | "warning" | "success" | "help" | "note" | "name" | "heading" | "dim";

/** A problem to report. With a location, it is shown with the source it
 * points at, like a compiler diagnostic. */
export interface Problem {
  code?: string;
  location?: Location;
  help?: string;
}

export interface Terminal {
  /** Whether stdin, stdout and stderr are terminals. */
  stdin: boolean;
  stdout: boolean;
  stderr: boolean;
  /** The terminal's width in columns, when stdout or stderr is one. */
  width: number | null;
  /** Whether colour is shown: false with NO_COLOR set. */
  color: boolean;
}

/** A file under a directory the plugin listed. */
export interface ListedFile {
  /** Relative to the directory listed, with forward slashes. */
  path: string;
  executable: boolean;
  /** A symbolic link, which is never followed. */
  symlink: boolean;
}

/** Fields for metadata: text, booleans, numbers, lists of text and nested
 * dictionaries. */
export type Fields = { [key: string]: string | boolean | number | string[] | Fields };

/** A file a build writes. */
export interface PlannedFile {
  /** Relative to the project root. */
  path: string;
  contents: string;
  /** What it was made from, for diagnostics. */
  origin?: string;
  /** A short description for `piton build --dry-run`. */
  label?: string;
  /** A name the editor's preview selects outputs by, like a Belay target id. */
  group?: string;
}

/** A file a build copies as it is, keeping its execute permission. */
export interface PlannedCopy {
  /** The file copied, relative to the project root. */
  from: string;
  /** Where it goes, relative to the project root. */
  to: string;
  origin?: string;
  label?: string;
  group?: string;
}

/** A problem a build found. */
export interface PlannedDiagnostic {
  severity: "error" | "warning";
  code: string;
  message: string;
  location?: Location;
  help?: string;
}

/** What a build would write. */
export interface Plan {
  files?: PlannedFile[];
  copies?: PlannedCopy[];
  diagnostics?: PlannedDiagnostic[];
  /** Labels recorded in the manifest, like each target's documentation date. */
  targets?: Record<string, string>;
  /** Where each anchor was placed, by name: the file and heading a
   * reference to it lands on. */
  locations?: Record<string, string>;
}

/** A file a finding changes, and its whole new contents. */
export interface Edit {
  /** Relative to the project root. */
  path: string;
  contents: string;
}

/** Something out of date in a project, and the edits that fix it. */
export interface Finding {
  message: string;
  edits: Edit[];
}

/** What an inspect hook found. */
export interface Inspection {
  findings: Finding[];
}

/** An anchor a reach hook names: the file that declares it, as an
 * anchor's `file` gives it or relative to the project root, and its name. */
export interface ReachedAnchor {
  file: string;
  name: string;
}

/** What a reach hook gives: the anchors the plugin reaches itself. */
export interface Reach {
  anchors: ReachedAnchor[];
}

/** Everything a plugin can do. Each method that reaches the machine is
 * checked against the capabilities the plugin declares. */
export interface Context {
  /** The plugin's name. */
  readonly plugin: string;
  readonly terminal: Terminal;
  project(): Project;
  /** The project's settings for this plugin: the anchor under `plugins` in
   * piton.config.pi implementing the plugin's settings anchor, if any. */
  settings(): Settings | null;
  /** Compiles the project. With `workspace`, every source under the root,
   * not only what the entry reaches. */
  compile(options?: { workspace?: boolean }): Compilation;
  /** What a plugin this one requires exports. */
  require(name: string): Record<string, unknown>;
  /** The marker each placeholder this plugin declares evaluates to. */
  placeholders(): Record<string, string>;
  /** A path from the file `from` to `to`, both relative to the project root. */
  relativePath(from: string, to: string): string;
  /** Encodes with the compiler's encoders. */
  encode(format: "frontmatter" | "yaml" | "toml", fields: Fields): string;
  encode(format: "tomlString" | "yamlScalar" | "jsonString", text: string): string;
  /** Starts a program the plugin declares under capabilities.process, in the
   * project root unless `cwd` says otherwise, with nothing on its stdin. */
  spawn(program: string, args: string[], options?: { cwd?: string; env?: Record<string, string> }): Process;
  /** Runs a program the plugin declares under capabilities.process with the
   * terminal: its input, its output and its prompts are the user's. Returns
   * its exit code once it ends, or null when a signal stopped it. */
  exec(program: string, args: string[], options?: { cwd?: string; env?: Record<string, string> }): number | null;
  /** Writes edits, for a command the user ran to make them, and returns the
   * paths changed. Each path has to be among capabilities.edits. */
  applyEdits(edits: Edit[]): string[];
  /** Asks the user to choose one of `options` on the terminal, and returns
   * the index chosen, or null when they cancel or no one can answer: stdin
   * or stdout isn't a terminal. Needs capabilities.interactive. */
  choose(message: string, options: string[]): number | null;
  /** The text of a file in the plugin's own directory, like a prompt it
   * ships with. Needs no capability: the files are the plugin's. */
  resource(path: string): string;
  /** Whether a program the plugin declares is on PATH. */
  onPath(program: string): boolean;
  /** Reads a file inside the project. Needs capabilities.files: read. */
  readFile(path: string): string;
  /** Every file under a directory inside the project, recursively. Needs
   * capabilities.files: read. */
  listFiles(path: string): ListedFile[];
  /** What is at a path inside the project, following symbolic links, or null
   * when nothing is. Needs capabilities.files: read. */
  stat(path: string): FileStatus | null;
  /** Writes a temporary file and returns its path. It is removed when the
   * command ends. Needs capabilities.temporaryFiles. */
  temporaryFile(contents: string, extension?: string): string;
  /** Resolves after `ms` milliseconds. */
  sleep(ms: number): Promise<void>;
  /** Text in a style. */
  paint(style: Style, text: string): string;
  /** Writes to stdout or stderr. Colour is removed where it isn't shown. */
  stdout(text: string): void;
  stderr(text: string): void;
  /** Writes to stderr exactly as given, terminal control codes included, for
   * redrawing a status line. Only meaningful when terminal.stderr is true. */
  stderrRaw(text: string): void;
  /** Problems are held back and printed after everything else, with the
   * count last; done and stopped print the command's outcome line. */
  report: {
    error(message: string, problem?: Problem): void;
    warning(message: string, problem?: Problem): void;
    help(message: string): void;
    note(message: string): void;
    done(message: string): void;
    stopped(message: string): void;
  };
}

/** Runs one command. Arguments arrive by name: a flag is a boolean, a
 * multiple argument a list, anything not given null. The number returned is
 * the exit code; nothing returned is 0. */
export type CommandHandler = (
  ctx: Context,
  args: Record<string, Json>,
) => number | void | Promise<number | void>;

export interface Plugin {
  /** A handler for each command the plugin's declaration lists. */
  commands?: Record<string, CommandHandler>;
  /** The plan of what `piton build` writes, for a plugin whose declaration
   * says `build: true`. */
  build?: (ctx: Context) => Plan | Promise<Plan>;
  /** Looks at the project before a command runs, for a plugin whose
   * declaration says `inspect: true`, and gives what is out of date. The
   * host shows the findings and asks before it applies any edit. */
  inspect?: (ctx: Context) => Inspection | Promise<Inspection>;
  /** Names the anchors the plugin reaches itself, for a plugin whose
   * declaration says `reach: true`: anchors it uses though nothing in the
   * specbase leads to them, like the tests the test plugin runs. `piton
   * reach` and the editor count them as roots beside the entry, so they are
   * neither unreachable nor unused. */
  reach?: (ctx: Context) => Reach | Promise<Reach>;
  /** What plugins that require this one can use. Each is a factory given
   * this plugin's own context, so it runs with this plugin's capabilities. */
  exports?: Record<string, (ctx: Context) => unknown>;
}

/** What a plugin's entry module exports as its default. */
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

/** Takes the host function, once, before any plugin is imported. Not part of
 * the API plugins use: a plugin that calls it gets an error, since the host
 * function is already taken. */
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
