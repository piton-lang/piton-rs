/**
 * Renderers as functions.
 *
 * The plugin has one default adapter, chosen when it is configured, because a
 * bare `import spec from './app.pi'` has to mean one thing. These are for the
 * other times: rendering a file a second way, or rendering it somewhere the
 * default is wrong.
 *
 * They run the compiler themselves, so they work anywhere Node does -- a build
 * script, a test, an Astro endpoint -- and not only inside a Vite transform.
 */

import { compile, type Adapter, type CompilerOptions } from './compiler.js';

export interface RenderOptions extends Partial<CompilerOptions> {}

async function render(file: string, adapter: Adapter, options: RenderOptions = {}) {
  const compiled = await compile(file, adapter, {
    binary: options.binary ?? 'piton',
    cwd: options.cwd ?? process.cwd(),
  });
  return compiled.value;
}

/** Renders a file as JSON, parsed. */
export async function json<T = unknown>(
  file: string,
  options: RenderOptions = {},
): Promise<T> {
  return JSON.parse(await render(file, 'json', options)) as T;
}

/** Renders a file as YAML, as text. */
export async function yaml(file: string, options: RenderOptions = {}): Promise<string> {
  return render(file, 'yaml', options);
}

/** Renders a file as Markdown, as text. */
export async function markdown(file: string, options: RenderOptions = {}): Promise<string> {
  return render(file, 'markdown', options);
}

export type { Adapter } from './compiler.js';
export { PitonError } from './compiler.js';
