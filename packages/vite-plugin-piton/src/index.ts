/**
 * A Vite plugin for Piton.
 *
 * `import spec from './app.pi'` compiles the file and hands back its anchors.
 * Editing that file, or any file it imports, reloads whatever imported it.
 */

import path from 'node:path';
import { compile, PitonError, type Adapter, type Compiled } from './compiler.js';

export interface PitonPluginOptions {
  /**
   * The adapter a bare `import` of a `.pi` file renders through.
   *
   * A bare import has to mean one thing, so this is the one. Import through
   * `virtual:piton/<adapter>/<file>`, or call a renderer directly, to get a
   * file rendered some other way.
   */
  adapter?: Adapter;
  /** Path to the `piton` binary, when it is not on `PATH`. */
  binary?: string;
}

const VIRTUAL = 'virtual:piton/';
const RESOLVED = '\0' + VIRTUAL;

/**
 * The part of Vite's plugin context these hooks use.
 *
 * Declared as an explicit `this` parameter rather than inferred, because a
 * method on an object literal has no `this` of its own and the hooks are
 * called with Vite's.
 */
interface PluginContext {
  addWatchFile(file: string): void;
}

/** Everything Vite may append to an id; the compiler wants the file alone. */
function withoutQuery(id: string): { file: string; query: string } {
  const mark = id.indexOf('?');
  return mark === -1
    ? { file: id, query: '' }
    : { file: id.slice(0, mark), query: id.slice(mark) };
}

function isPiton(file: string): boolean {
  return file.endsWith('.pi');
}

/**
 * Generates the module a compiled file becomes.
 *
 * JSON gives an object, so the anchors are reachable as properties and the
 * example in the specification -- `spec.anchors.SaveButton` -- works. The text
 * adapters have no structure to expose, so the module is the text.
 */
function moduleFor(compiled: Compiled, adapter: Adapter): string {
  const text = JSON.stringify(compiled.value);
  if (adapter !== 'json') {
    return `export const text = ${text};\nexport default text;\n`;
  }
  // Parsed here rather than in the browser, so a malformed render fails the
  // build instead of the page.
  const anchors = JSON.stringify(JSON.parse(compiled.value));
  return (
    `export const anchors = ${anchors};\n` +
    `export const text = ${text};\n` +
    `export default { anchors, text };\n`
  );
}

export default function piton(options: PitonPluginOptions = {}): unknown {
  const adapter: Adapter = options.adapter ?? 'json';
  const binary = options.binary ?? 'piton';
  let root = process.cwd();

  /**
   * Which modules were built from which files.
   *
   * Vite tracks the imports it can see in JavaScript. It cannot see one `.pi`
   * file importing another, and it cannot see inside a virtual module at all,
   * so that edge is recorded here when the compiler reports it.
   */
  const dependents = new Map<string, Set<string>>();

  function track(id: string, compiled: Compiled) {
    for (const dependency of compiled.dependencies) {
      const absolute = path.resolve(root, dependency);
      let ids = dependents.get(absolute);
      if (!ids) {
        ids = new Set();
        dependents.set(absolute, ids);
      }
      ids.add(id);
    }
  }

  return {
    name: 'vite-plugin-piton',
    // Ahead of the other plugins, so `.pi` is compiled before anything else
    // tries to read it as JavaScript.
    enforce: 'pre' as const,

    configResolved(config: { root: string }) {
      root = config.root;
    },

    async resolveId(source: string, importer: string | undefined) {
      if (source.startsWith(VIRTUAL)) {
        return RESOLVED + source.slice(VIRTUAL.length);
      }
      const { file, query } = withoutQuery(source);
      if (!isPiton(file)) {
        return null;
      }
      // Relative to the file that imported it, like any other import.
      const base = importer ? path.dirname(importer) : root;
      const resolved = path.isAbsolute(file) ? file : path.resolve(base, file);
      return resolved + query;
    },

    async load(this: PluginContext, id: string) {
      if (id.startsWith(RESOLVED)) {
        // `virtual:piton/<adapter>/<file>` renders one file a second way,
        // without needing a second file to import.
        const rest = id.slice(RESOLVED.length);
        const slash = rest.indexOf('/');
        const named = slash === -1 ? '' : rest.slice(0, slash);
        if (named !== 'json' && named !== 'yaml' && named !== 'markdown') {
          throw new Error(
            `\`${VIRTUAL}${rest}\` names no adapter. Write ` +
              `\`${VIRTUAL}<json|yaml|markdown>/<file>\`.`,
          );
        }
        const file = path.resolve(root, rest.slice(slash + 1));
        const compiled = await compile(file, named, { binary, cwd: root });
        track(id, compiled);
        for (const dependency of compiled.dependencies) {
          this.addWatchFile(path.resolve(root, dependency));
        }
        return moduleFor(compiled, named);
      }

      const { file } = withoutQuery(id);
      if (!isPiton(file)) {
        return null;
      }
      const compiled = await compile(file, adapter, { binary, cwd: root });
      track(id, compiled);
      // Vite watches a file it can see being imported. The rest of the module
      // graph is invisible to it, so each dependency is named here.
      for (const dependency of compiled.dependencies) {
        this.addWatchFile(path.resolve(root, dependency));
      }
      return moduleFor(compiled, adapter);
    },

    handleHotUpdate(context: {
      file: string;
      server: { moduleGraph: { getModuleById(id: string): unknown } };
      modules: unknown[];
    }) {
      if (!isPiton(context.file)) {
        return;
      }
      // An edit to an imported file has to reload the files that import it,
      // not only the one that changed.
      const affected = dependents.get(path.resolve(context.file));
      if (!affected) {
        return;
      }
      const reload = [...context.modules];
      for (const id of affected) {
        const module = context.server.moduleGraph.getModuleById(id);
        if (module && !reload.includes(module)) {
          reload.push(module);
        }
      }
      return reload;
    },
  };
}

export { PitonError };
export type { Adapter, Compiled } from './compiler.js';
