// @piton/plugin: what a plugin's code can do.
//
// Bundled with the Piton compiler and importable only from inside a plugin.
// Everything here is a thin, typed layer over one host function, `__piton`,
// which takes an operation and its arguments as JSON. Operations that wait,
// like reading a process's output, return promises the host resolves from
// its event loop, so a plugin can run many processes at once with plain
// async code.

declare const __piton: (operation: string, payload: string) => unknown;

function call<T>(operation: string, payload: unknown = null): T {
  const result = __piton(operation, JSON.stringify(payload));
  return (typeof result === "string" ? JSON.parse(result) : null) as T;
}

async function callAsync<T>(operation: string, payload: unknown = null): Promise<T> {
  const result = await (__piton(operation, JSON.stringify(payload)) as Promise<string>);
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
  /** Belay's codeRoot, when the project configures Belay. */
  codeRoot: string | null;
  /** The Belay adapters the project builds, like claude-code. */
  adapters: string[];
}

/** An anchor read from a compilation, or a project's settings for a plugin. */
export interface Anchor {
  name: string;
  file: string;
  exported: boolean;
  abstract: boolean;
  keyword: string;
  properties: Record<string, Json>;
  /** Where the anchor names a property itself, else where its name is. */
  location(property?: string): Location;
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
  properties: Record<string, Json>;
}

/** A compiled project. */
export class Compilation {
  constructor(
    private readonly handle: number,
    /** How many errors compiling found. */
    readonly errors: number,
    /** How many warnings compiling found. */
    readonly warnings: number,
  ) {}

  /** The anchors matching a query, ordered by file, then name. */
  anchors(query: AnchorQuery = {}): Anchor[] {
    const handle = this.handle;
    return call<AnchorRecord[]>("anchors", { handle, query }).map((record) => ({
      name: record.name,
      file: record.file,
      exported: record.exported,
      abstract: record.abstract,
      keyword: record.keyword,
      properties: record.properties,
      location: (property?: string) =>
        call<Location>("location", { handle, id: record.id, property: property ?? null }),
    }));
  }

  /** Prints the compilation's diagnostics as `piton check` does, with the
   * count last, and says whether any were errors. */
  reportDiagnostics(): boolean {
    return call<boolean>("diagnostics", { handle: this.handle });
  }
}

/** One line a process wrote. */
export interface Line {
  stream: "stdout" | "stderr";
  line: string;
}

/** A running process. Its output is read line by line as it is written. */
export class Process {
  constructor(private readonly id: number) {}

  /** The next line it wrote, or null once it has exited and every line has
   * been read. */
  next(): Promise<Line | null> {
    return callAsync<Line | null>("next", { id: this.id });
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
    return callAsync<number | null>("wait", { id: this.id });
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

/** An agentic tool `piton agent` can launch. */
export interface AgentTool {
  name: string;
  program: string;
  target: string;
}

export interface Terminal {
  /** Whether stdout and stderr are terminals. */
  stdout: boolean;
  stderr: boolean;
  /** The terminal's width in columns, when stdout or stderr is one. */
  width: number | null;
  /** Whether colour is shown: false with NO_COLOR set. */
  color: boolean;
}

/** Everything a command can do. Each method is checked against the
 * capabilities the plugin declares: anything else is refused. */
export interface Context {
  /** The plugin's name. */
  readonly plugin: string;
  readonly terminal: Terminal;
  project(): Project;
  /** The project's settings for this plugin: the anchor under `plugins` in
   * piton.config.pi implementing the plugin's settings anchor, if any. */
  settings(): Anchor | null;
  /** Compiles the project. With `workspace`, every source under the root,
   * not only what the entry reaches. */
  compile(options?: { workspace?: boolean }): Compilation;
  /** Starts a program the plugin declares under capabilities.process, in the
   * project root unless `cwd` says otherwise, with nothing on its stdin. */
  spawn(program: string, args: string[], options?: { cwd?: string; env?: Record<string, string> }): Process;
  /** Whether a program the plugin declares is on PATH. */
  onPath(program: string): boolean;
  /** Reads a file inside the project. Needs capabilities.files: read. */
  readFile(path: string): string;
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
  /** The agentic tools Piton knows, and the fluency prompt each is given. */
  agents: {
    list(): AgentTool[];
    fluency(name: string): string;
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
  commands: Record<string, CommandHandler>;
}

/** What a plugin's entry module exports as its default. */
export function definePlugin(plugin: Plugin): Plugin {
  return plugin;
}

const context: Context = {
  plugin: call<string>("plugin"),
  terminal: call<Terminal>("terminal"),
  project: () => call<Project>("project"),
  settings() {
    const record = call<(AnchorRecord & { spans: Record<string, Location> }) | null>("settings");
    if (record === null) return null;
    return {
      name: record.name,
      file: record.file,
      exported: record.exported,
      abstract: record.abstract,
      keyword: record.keyword,
      properties: record.properties,
      location: (property?: string) =>
        (property !== undefined && record.spans[property]) || record.spans[""],
    };
  },
  compile(options = {}) {
    const compiled = call<{ handle: number; errors: number; warnings: number }>("compile", {
      workspace: options.workspace ?? false,
    });
    return new Compilation(compiled.handle, compiled.errors, compiled.warnings);
  },
  spawn: (program, args, options = {}) =>
    new Process(call<number>("spawn", { program, args, cwd: options.cwd ?? null, env: options.env ?? {} })),
  onPath: (program) => call<boolean>("onPath", { program }),
  readFile: (path) => call<string>("readFile", { path }),
  temporaryFile: (contents, extension = "") => call<string>("temporaryFile", { contents, extension }),
  sleep: (ms) => callAsync<null>("sleep", { ms }).then(() => undefined),
  paint: (style, text) => call<string>("paint", { style, text }),
  stdout: (text) => call("write", { stream: "stdout", text }),
  stderr: (text) => call("write", { stream: "stderr", text }),
  stderrRaw: (text) => call("write", { stream: "stderrRaw", text }),
  report: {
    error: (message, problem = {}) => call("report", { kind: "error", message, ...problem }),
    warning: (message, problem = {}) => call("report", { kind: "warning", message, ...problem }),
    help: (message) => call("report", { kind: "help", message }),
    note: (message) => call("report", { kind: "note", message }),
    done: (message) => call("report", { kind: "done", message }),
    stopped: (message) => call("report", { kind: "stopped", message }),
  },
  agents: {
    list: () => call<AgentTool[]>("agents"),
    fluency: (name) => call<string>("fluency", { name }),
  },
};

/** Runs a command for the host. Not part of the API plugins use. */
export async function __run(plugin: Plugin, command: string, args: string): Promise<number> {
  const handler = plugin?.commands?.[command];
  if (typeof handler !== "function") {
    throw new Error(`the plugin has no handler for its command \`${command}\``);
  }
  const code = await handler(context, JSON.parse(args));
  return typeof code === "number" ? code : 0;
}
