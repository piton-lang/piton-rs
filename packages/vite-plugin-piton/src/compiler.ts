/**
 * Running the Piton compiler.
 *
 * The compiler is the `piton` binary rather than a reimplementation in
 * TypeScript. Anything else would be a second implementation of the language
 * that could disagree with the first, and the disagreement would surface as a
 * build that succeeds while `piton check` fails.
 */

import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

import type { Renderer } from './renderers.js';

const run = promisify(execFile);

export type { Renderer } from './renderers.js';

/** @deprecated Renamed to {@link Renderer}, after `piton compile --renderer`. */
export type Adapter = Renderer;

/** A compiled file, with the sources it was built from. */
export interface Compiled {
  /** The rendered output, as text in the renderer's own format. */
  value: string;
  /**
   * Every `.pi` file the compilation read, including the entry, as absolute
   * paths.
   *
   * Imports resolve through the module graph, so a file can depend on one the
   * importing text never names. Watching these is what makes an edit to an
   * imported file reload the importer.
   */
  dependencies: string[];
}

export interface CompilerOptions {
  /** Path to the `piton` binary. */
  binary: string;
  /** Directory to run in, so a relative `piton.config.pi` resolves. */
  cwd: string;
}

/**
 * Raised when the compiler reports a problem.
 *
 * The compiler's own diagnostics are the message. They name the file, line and
 * cause already, and rewriting them here would only make them worse.
 */
export class PitonError extends Error {
  constructor(
    readonly file: string,
    readonly diagnostics: string,
  ) {
    super(diagnostics.trim() || `piton failed on ${file}`);
    this.name = 'PitonError';
  }
}

/**
 * Compilers that predate `--renderer` and only know it as `--adapter`, by
 * binary path, so the retry below happens once per binary rather than once per
 * file.
 */
const legacyFlag = new Set<string>();

function rejectsRendererFlag(stderr: string): boolean {
  return /--renderer/.test(stderr) && /unexpected|unrecognized|unknown|wasn't expected/i.test(stderr);
}

async function invoke(
  file: string,
  renderer: Renderer,
  options: CompilerOptions,
): Promise<string> {
  const flag = legacyFlag.has(options.binary) ? '--adapter' : '--renderer';
  try {
    const result = await run(
      options.binary,
      ['compile', flag, renderer, '--dependencies', file],
      { cwd: options.cwd, maxBuffer: 64 * 1024 * 1024 },
    );
    return result.stdout;
  } catch (error) {
    const failure = error as { stderr?: string; stdout?: string; code?: string };
    if (failure.code === 'ENOENT') {
      throw new PitonError(
        file,
        `cannot run \`${options.binary}\`. Install the Piton compiler, or set ` +
          `the \`binary\` option to its path.`,
      );
    }
    if (flag === '--renderer' && rejectsRendererFlag(failure.stderr ?? '')) {
      legacyFlag.add(options.binary);
      return invoke(file, renderer, options);
    }
    throw new PitonError(file, failure.stderr || failure.stdout || String(error));
  }
}

/** Compiles one file through a renderer, and reports what it read. */
export async function compile(
  file: string,
  renderer: Renderer,
  options: CompilerOptions,
): Promise<Compiled> {
  const stdout = await invoke(file, renderer, options);
  let parsed: Compiled;
  try {
    parsed = JSON.parse(stdout) as Compiled;
  } catch {
    throw new PitonError(file, `the compiler returned output that is not JSON:\n${stdout}`);
  }
  if (typeof parsed?.value !== 'string' || !Array.isArray(parsed.dependencies)) {
    throw new PitonError(
      file,
      `the compiler's \`--dependencies\` output has no \`value\` string and ` +
        `\`dependencies\` list:\n${stdout}`,
    );
  }
  // Printed plainly, a compiled file ends in a newline; wrapped as `value` it
  // may not. The two should read the same, so the text always ends in one.
  if (parsed.value !== '' && !parsed.value.endsWith('\n')) {
    parsed.value += '\n';
  }
  return parsed;
}
