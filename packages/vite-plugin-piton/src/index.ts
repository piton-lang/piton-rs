/**
 * A Vite plugin for Piton.
 *
 * ```ts
 * import { SaveButton } from '../spec/app.pi';
 * import spec from '../spec/app.pi';
 *
 * const cancel = spec.CancelButton;
 * ```
 *
 * Each export of the Piton file is a named export of the module, and the
 * default export is the whole file, the same as what `piton compile` gives.
 * Editing that file, or any file it imports, reloads whatever imported it.
 */

import path from 'node:path';
import { compile, PitonError, type Compiled } from './compiler.js';
import { isRenderer, markdown, yaml, type Renderer } from './renderers.js';

export interface PitonPluginOptions {
  /**
   * The renderer a bare `import` of a `.pi` file goes through: `json` (the
   * default), `yaml` or `markdown`.
   *
   * With `json` the default export is the compiled object and each named
   * export is that export's value. With `yaml` or `markdown` the default export
   * is the rendered file as text -- what `piton compile --renderer <renderer>`
   * prints -- and each named export is that export rendered the same way.
   *
   * Import through `virtual:piton/<renderer>/<file>`, or call a renderer from
   * `vite-plugin-piton/renderers`, to render something another way.
   */
  renderer?: Renderer;
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

const RESERVED = new Set(
  (
    'await break case catch class const continue debugger default delete do else enum ' +
    'export extends false finally for function if implements import in instanceof ' +
    'interface let new null package private protected public return static super ' +
    'switch this throw true try typeof var void while with yield arguments eval'
  ).split(' '),
);

/** Whether `name` can be written as `export const <name>`. */
function isIdentifier(name: string): boolean {
  return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) && !RESERVED.has(name);
}

/** What a compiled file renders to, before it becomes a module. */
interface Rendered {
  /** The compiled value: one key per export. */
  exports: Record<string, unknown>;
  /** The whole file through the renderer, when it is not JSON. */
  text?: string;
}

/**
 * Generates the module a compiled file becomes.
 *
 * Each export is a named export. A name that is not a JavaScript identifier --
 * a kebab-case export, say -- is still exported, under its own name as a
 * string (`export { x as "my-name" }`), which `import { "my-name" as x }`
 * reads. An export called `default` is only reachable through the default
 * export, since the name is taken.
 */
export function moduleFor(rendered: Rendered, renderer: Renderer): string {
  const lines: string[] = [];
  const file = '__piton_file';
  if (renderer === 'json') {
    lines.push(`const ${file} = ${JSON.stringify(rendered.exports)};`);
  } else {
    lines.push(`const ${file} = ${JSON.stringify(rendered.text ?? '')};`);
  }
  lines.push(`export default ${file};`);

  let counter = 0;
  for (const [name, value] of Object.entries(rendered.exports)) {
    if (name === 'default') {
      continue;
    }
    let expression: string;
    if (renderer === 'json') {
      expression = `${file}[${JSON.stringify(name)}]`;
    } else if (renderer === 'yaml') {
      expression = JSON.stringify(yaml(value));
    } else {
      expression = JSON.stringify(markdown(value, { title: name }));
    }
    if (isIdentifier(name) && !name.startsWith('__piton_')) {
      lines.push(`export const ${name} = ${expression};`);
    } else {
      const local = `__piton_export_${counter++}`;
      lines.push(`const ${local} = ${expression};`, `export { ${local} as ${JSON.stringify(name)} };`);
    }
  }
  return lines.join('\n') + '\n';
}

export default function piton(options: PitonPluginOptions = {}) {
  const renderer: Renderer = options.renderer ?? 'json';
  if (!isRenderer(renderer)) {
    throw new Error(
      `vite-plugin-piton: \`${String(renderer)}\` is not a renderer. Use json, yaml or markdown.`,
    );
  }
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

  function track(context: PluginContext, id: string, dependencies: string[]) {
    for (const dependency of dependencies) {
      const absolute = path.resolve(root, dependency);
      let ids = dependents.get(absolute);
      if (!ids) {
        ids = new Set();
        dependents.set(absolute, ids);
      }
      ids.add(id);
      // Vite watches a file it can see being imported. The rest of the module
      // graph is invisible to it, so each dependency is named here.
      context.addWatchFile(absolute);
    }
  }

  /**
   * Compiles a file for one renderer. The exports always come from the JSON
   * render, since the named exports need the structure; a text renderer adds
   * a second run for the default export, so that it is exactly what the
   * compiler prints.
   */
  async function build(context: PluginContext, id: string, file: string, as: Renderer) {
    const runs: Promise<Compiled>[] = [compile(file, 'json', { binary, cwd: root })];
    if (as !== 'json') {
      runs.push(compile(file, as, { binary, cwd: root }));
    }
    const [data, text] = await Promise.all(runs);
    // Parsed here rather than in the browser, so a malformed render fails the
    // build instead of the page.
    let exports: Record<string, unknown>;
    try {
      exports = JSON.parse(data.value) as Record<string, unknown>;
    } catch {
      throw new PitonError(file, `the compiler returned JSON that does not parse:\n${data.value}`);
    }
    track(context, id, [...data.dependencies, ...(text?.dependencies ?? [])]);
    return moduleFor({ exports, text: text?.value }, as);
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
      const base = importer && !importer.startsWith('\0') ? path.dirname(importer) : root;
      const resolved = path.isAbsolute(file) ? file : path.resolve(base, file);
      return resolved + query;
    },

    async load(this: PluginContext, id: string) {
      if (id.startsWith(RESOLVED)) {
        // `virtual:piton/<renderer>/<file>` renders one file a second way,
        // without needing a second file to import.
        const rest = id.slice(RESOLVED.length);
        const slash = rest.indexOf('/');
        const named = slash === -1 ? '' : rest.slice(0, slash);
        if (!isRenderer(named)) {
          throw new Error(
            `\`${VIRTUAL}${rest}\` names no renderer. Write ` +
              `\`${VIRTUAL}<json|yaml|markdown>/<file>\`.`,
          );
        }
        const file = path.resolve(root, rest.slice(slash + 1));
        return build(this, id, file, named);
      }

      const { file } = withoutQuery(id);
      if (!isPiton(file)) {
        return null;
      }
      return build(this, id, file, renderer);
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
export type { Adapter, Compiled, CompilerOptions } from './compiler.js';
export { compileFile, renderFile, type RenderFileOptions } from './files.js';
export {
  json,
  yaml,
  markdown,
  render,
  renderers,
  titleCase,
  type Renderer,
  type PitonValue,
  type JsonOptions,
  type MarkdownOptions,
} from './renderers.js';
