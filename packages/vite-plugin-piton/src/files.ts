/**
 * Rendering files through the compiler.
 *
 * The functions in `vite-plugin-piton/renderers` render a value that is
 * already in hand. These render a `.pi` file by running `piton compile`, so
 * the output is exactly what the compiler writes. They work anywhere Node
 * does -- a build script, a test, an Astro endpoint -- and not only inside a
 * Vite transform.
 */

import { compile, type CompilerOptions } from './compiler.js';
import type { Renderer } from './renderers.js';

export interface RenderFileOptions extends Partial<CompilerOptions> {}

function compilerOptions(options: RenderFileOptions): CompilerOptions {
  return {
    binary: options.binary ?? 'piton',
    cwd: options.cwd ?? process.cwd(),
  };
}

/** Renders a file through a renderer, as text: `piton compile --renderer <renderer>`. */
export async function renderFile(
  file: string,
  renderer: Renderer = 'json',
  options: RenderFileOptions = {},
): Promise<string> {
  const compiled = await compile(file, renderer, compilerOptions(options));
  return compiled.value;
}

/** Compiles a file to its value: an object with one key per export. */
export async function compileFile<T = Record<string, unknown>>(
  file: string,
  options: RenderFileOptions = {},
): Promise<T> {
  return JSON.parse(await renderFile(file, 'json', options)) as T;
}

/** Compiles a file as JSON, parsed. */
export const json = compileFile;

/** Renders a file as YAML text. */
export function yaml(file: string, options: RenderFileOptions = {}): Promise<string> {
  return renderFile(file, 'yaml', options);
}

/** Renders a file as Markdown text. */
export function markdown(file: string, options: RenderFileOptions = {}): Promise<string> {
  return renderFile(file, 'markdown', options);
}

export { PitonError } from './compiler.js';
export type { Renderer } from './renderers.js';
