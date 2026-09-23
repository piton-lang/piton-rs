/**
 * Renderers as functions.
 *
 * The plugin renders a bare `import` through one default renderer, chosen when
 * it is configured. These are the same renderers as plain functions, to use
 * inline on any value -- the whole file, or one export of it:
 *
 * ```ts
 * import { SaveButton } from '../spec/app.pi';
 * import { markdown, yaml } from 'vite-plugin-piton/renderers';
 *
 * const doc = markdown(SaveButton, { title: 'SaveButton' });
 * ```
 *
 * They are pure functions over the compiled value (what `piton compile` gives
 * as JSON), with no Node imports, so they run in the browser as well as in a
 * build script. They follow the compiler's own renderers, with one limit: the
 * compiled JSON no longer says which objects were anchors and which were
 * dictionaries, or which lists were implicit mixed blocks, so the Markdown
 * renderer treats every nested object as a dictionary. Render the file through
 * the compiler (`renderFile` from `vite-plugin-piton`) when the output has to
 * match `piton compile --renderer markdown` exactly.
 */

/** Output formats a value can be rendered through. */
export type Renderer = 'json' | 'yaml' | 'markdown';

/** A compiled Piton value: what `piton compile` produces as JSON. */
export type PitonValue =
  | string
  | number
  | boolean
  | null
  | PitonValue[]
  | { [key: string]: PitonValue };

type PitonObject = { [key: string]: PitonValue };

function isObject(value: unknown): value is PitonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * A reference serializes as `{"$ref": "path#Name"}` rather than an embedded
 * copy, so the reader can tell the two apart.
 */
function referenceName(value: unknown): string | undefined {
  if (!isObject(value)) {
    return undefined;
  }
  const keys = Object.keys(value);
  if (keys.length !== 1 || keys[0] !== '$ref' || typeof value.$ref !== 'string') {
    return undefined;
  }
  const target = value.$ref;
  const hash = target.lastIndexOf('#');
  return hash === -1 ? target : target.slice(hash + 1);
}

/** Numbers are written the way the compiler writes them. */
function formatNumber(n: number): string {
  if (Number.isNaN(n)) {
    return 'null';
  }
  if (!Number.isFinite(n)) {
    return n > 0 ? 'Infinity' : '-Infinity';
  }
  return String(n);
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

export interface JsonOptions {
  /** Spaces per level. The compiler uses 2. */
  indent?: number;
}

/** Renders a value as JSON text, laid out the way `piton compile` lays it out. */
export function json(value: unknown, options: JsonOptions = {}): string {
  const step = options.indent ?? 2;
  const write = (item: unknown, depth: number): string => {
    if (item === null || item === undefined) {
      return 'null';
    }
    if (typeof item === 'number') {
      return Number.isFinite(item) ? formatNumber(item) : 'null';
    }
    if (typeof item === 'boolean' || typeof item === 'string') {
      return JSON.stringify(item);
    }
    const pad = ' '.repeat((depth + 1) * step);
    const close = ' '.repeat(depth * step);
    if (Array.isArray(item)) {
      if (item.length === 0) {
        return '[]';
      }
      return `[\n${item.map((entry) => pad + write(entry, depth + 1)).join(',\n')}\n${close}]`;
    }
    if (isObject(item)) {
      if (referenceName(item) !== undefined) {
        // Written on one line, as the compiler writes a reference.
        return `{"$ref": ${JSON.stringify(item.$ref)}}`;
      }
      const entries = Object.entries(item);
      if (entries.length === 0) {
        return '{}';
      }
      return `{\n${entries
        .map(([key, entry]) => `${pad}${JSON.stringify(key)}: ${write(entry, depth + 1)}`)
        .join(',\n')}\n${close}}`;
    }
    return JSON.stringify(String(item));
  };
  return write(value, 0) + '\n';
}

// ---------------------------------------------------------------------------
// YAML
// ---------------------------------------------------------------------------

const YAML_KEY_SPECIAL = /[\s:#{}[\],&*!|>'"%@`]/;
const YAML_LEADING = /^[-?:,[\]{}#&*!|>'"%@`]/;
const YAML_WORDS = new Set(['true', 'false', 'null', 'yes', 'no', 'on', 'off', '~']);

/** Whether the text reads as a number, which YAML would take it for. */
function looksNumeric(text: string): boolean {
  return (
    /^[+-]?(inf|infinity|nan)$/i.test(text) ||
    /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(text)
  );
}

/**
 * Quotes a scalar only when leaving it bare would change how YAML reads it,
 * which keeps prose readable while staying unambiguous.
 */
function yamlQuote(text: string): string {
  const needsQuotes =
    text === '' ||
    text.includes('\n') ||
    text.startsWith(' ') ||
    text.endsWith(' ') ||
    YAML_LEADING.test(text) ||
    text.includes(': ') ||
    text.endsWith(':') ||
    text.includes(' #') ||
    YAML_WORDS.has(text) ||
    looksNumeric(text);
  if (!needsQuotes) {
    return text;
  }
  let out = '"';
  for (const ch of text) {
    switch (ch) {
      case '"':
        out += '\\"';
        break;
      case '\\':
        out += '\\\\';
        break;
      case '\n':
        out += '\\n';
        break;
      case '\r':
        out += '\\r';
        break;
      case '\t':
        out += '\\t';
        break;
      default:
        out += ch;
    }
  }
  return out + '"';
}

function yamlKey(key: string): string {
  return key === '' || YAML_KEY_SPECIAL.test(key) ? yamlQuote(key) : key;
}

function yamlScalar(value: unknown): string {
  if (value === null || value === undefined) {
    return 'null';
  }
  if (typeof value === 'boolean') {
    return value ? 'true' : 'false';
  }
  if (typeof value === 'number') {
    return formatNumber(value);
  }
  const ref = isObject(value) ? value.$ref : undefined;
  if (typeof ref === 'string') {
    return yamlQuote(`!ref ${ref}`);
  }
  return yamlQuote(String(value));
}

function yamlMap(map: PitonObject, indent: number, out: string[]): void {
  const pad = ' '.repeat(indent);
  const entries = Object.entries(map);
  if (entries.length === 0) {
    out.push(`${pad}{}`);
    return;
  }
  for (const [key, value] of entries) {
    const head = `${pad}${yamlKey(key)}`;
    if (isObject(value) && referenceName(value) === undefined) {
      if (Object.keys(value).length === 0) {
        out.push(`${head}: {}`);
      } else {
        out.push(`${head}:`);
        yamlMap(value, indent + 2, out);
      }
    } else if (Array.isArray(value)) {
      if (value.length === 0) {
        out.push(`${head}: []`);
      } else {
        out.push(`${head}:`);
        // A list under a key sits at the key's own indentation.
        yamlList(value, indent, out);
      }
    } else {
      out.push(`${head}: ${yamlScalar(value)}`);
    }
  }
}

function yamlList(items: PitonValue[], indent: number, out: string[]): void {
  const pad = ' '.repeat(indent);
  for (const item of items) {
    if (isObject(item) && referenceName(item) === undefined) {
      if (Object.keys(item).length === 0) {
        out.push(`${pad}- {}`);
        continue;
      }
      const nested: string[] = [];
      yamlMap(item, indent + 2, nested);
      // The first key moves onto the bullet's line.
      out.push(`${pad}- ${nested[0].trimStart()}`, ...nested.slice(1));
    } else if (Array.isArray(item)) {
      if (item.length === 0) {
        out.push(`${pad}- []`);
      } else {
        out.push(`${pad}-`);
        yamlList(item, indent + 2, out);
      }
    } else {
      out.push(`${pad}- ${yamlScalar(item)}`);
    }
  }
}

/** Renders a value as a YAML document, the way `piton compile` writes one. */
export function yaml(value: unknown): string {
  const out: string[] = [];
  if (isObject(value) && referenceName(value) === undefined) {
    yamlMap(value, 0, out);
  } else if (Array.isArray(value)) {
    yamlList(value as PitonValue[], 0, out);
  } else {
    out.push(yamlScalar(value));
  }
  return out.join('\n') + '\n';
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

/** Markdown has six heading levels; past that the hierarchy uses bold labels. */
const MAX_HEADING_LEVEL = 6;

/**
 * Splits a source name into words the way the compiler does: at a lowercase
 * letter or digit followed by an uppercase letter, at a letter followed by a
 * digit, and at `-`, `_` and spaces. Runs of capitals stay together, so
 * `whatIsAType` becomes `What Is AType`.
 */
export function splitWords(name: string): string[] {
  const words: string[] = [];
  let current = '';
  const chars = [...name];
  chars.forEach((ch, index) => {
    if (ch === '-' || ch === '_' || ch === ' ') {
      if (current) {
        words.push(current);
        current = '';
      }
      return;
    }
    const previous = index > 0 ? chars[index - 1] : undefined;
    const isLower = (c: string) => c !== c.toUpperCase() && c === c.toLowerCase();
    const isUpper = (c: string) => c !== c.toLowerCase() && c === c.toUpperCase();
    const isDigit = (c: string) => c >= '0' && c <= '9';
    const boundary =
      previous !== undefined &&
      (isLower(previous) || isDigit(previous)) &&
      (isUpper(ch) || (isDigit(ch) && !isDigit(previous)));
    if (boundary && current) {
      words.push(current);
      current = '';
    }
    current += ch;
  });
  if (current) {
    words.push(current);
  }
  return words;
}

/** `myProperty` becomes `My Property`. */
export function titleCase(name: string): string {
  const words = splitWords(name);
  if (words.length === 0) {
    return name;
  }
  return words.map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(' ');
}

function heading(level: number, title: string): string {
  return level <= MAX_HEADING_LEVEL ? `${'#'.repeat(level)} ${title}\n` : `**${title}**\n`;
}

function isSimple(value: unknown): boolean {
  return value === null || ['string', 'number', 'boolean'].includes(typeof value) || referenceName(value) !== undefined;
}

/** A dictionary holding only scalars and other such dictionaries: data. */
function isPureDictionary(value: unknown): value is PitonObject {
  return (
    isObject(value) &&
    referenceName(value) === undefined &&
    Object.values(value).every((entry) => isSimple(entry) || isPureDictionary(entry))
  );
}

function mdScalar(value: unknown): string {
  if (value === null || value === undefined) {
    return 'null';
  }
  if (typeof value === 'boolean') {
    return value ? 'true' : 'false';
  }
  if (typeof value === 'number') {
    return formatNumber(value);
  }
  const name = referenceName(value);
  if (name !== undefined) {
    return name;
  }
  return String(value);
}

/** Every line after the first moves in, so a value stays inside its entry. */
function indentContinuations(text: string, indent: number): string {
  const pad = ' '.repeat(indent);
  return text
    .split('\n')
    .map((line, index) => (index === 0 ? line : pad + line))
    .join('\n');
}

/** A dictionary as indentation-based `key: value` lines. */
function mdMap(map: PitonObject, indent: number): string {
  const pad = ' '.repeat(indent);
  const lines: string[] = [];
  for (const [name, value] of Object.entries(map)) {
    if (isObject(value) && referenceName(value) === undefined) {
      if (Object.keys(value).length === 0) {
        lines.push(`${pad}${name}: `);
      } else {
        lines.push(`${pad}${name}:`, mdMap(value, indent + 2));
      }
    } else if (Array.isArray(value)) {
      lines.push(`${pad}${name}:`);
      if (value.length > 0) {
        lines.push(mdList(value, indent + 2));
      }
    } else {
      lines.push(`${pad}${name}: ${indentContinuations(mdScalar(value), indent + 2)}`);
    }
  }
  return lines.join('\n');
}

/** A list as a Markdown list, nested structure indented beneath it. */
function mdList(items: PitonValue[], indent: number): string {
  const pad = ' '.repeat(indent);
  const lines: string[] = [];
  for (const item of items) {
    if (isObject(item) && referenceName(item) === undefined && Object.keys(item).length > 0) {
      // The first key shares the bullet's line.
      lines.push(`${pad}- ${mdMap(item, indent + 2).trimStart()}`);
    } else if (Array.isArray(item) && item.length > 0) {
      lines.push(mdList(item, indent + 2));
    } else if (isObject(item) && referenceName(item) === undefined) {
      lines.push(`${pad}- `);
    } else if (Array.isArray(item)) {
      lines.push(`${pad}- `);
    } else {
      lines.push(`${pad}- ${indentContinuations(mdScalar(item), indent + 2)}`);
    }
  }
  return lines.join('\n');
}

function paragraph(text: string, out: string[]): void {
  if (text !== '') {
    out.push(text, '\n\n');
  }
}

function mdProperties(map: PitonObject, level: number, out: string[]): void {
  for (const [name, value] of Object.entries(map)) {
    out.push(heading(level, titleCase(name)), '\n');
    mdValue(value, level, out);
  }
}

/** A value at document level, where headings are available. */
function mdValue(value: unknown, level: number, out: string[]): void {
  if (Array.isArray(value)) {
    if (value.length > 0) {
      paragraph(mdList(value, 0), out);
    }
  } else if (isPureDictionary(value)) {
    // Data rather than document structure: it reads better fenced than as a
    // run of near-empty headings.
    if (Object.keys(value).length > 0) {
      paragraph('```\n' + mdMap(value, 0) + '\n```', out);
    }
  } else if (isObject(value) && referenceName(value) === undefined) {
    mdProperties(value, level + 1, out);
  } else {
    paragraph(mdScalar(value), out);
  }
}

export interface MarkdownOptions {
  /**
   * A title for the document, written as the level-`level` heading, with the
   * value's properties one level below it. Pass an export's name here to get
   * the same section `piton compile --renderer markdown` writes for it.
   */
  title?: string;
  /** The heading level to start at. Defaults to 1. */
  level?: number;
}

/**
 * Renders a value as Markdown, following the compiler's Markdown rules:
 * property names become title-cased headings (`myProperty` is `My Property`),
 * heading levels carry the hierarchy and turn into bold labels past level six,
 * lists are Markdown lists, a dictionary of only scalars and dictionaries is
 * fenced as indented `key: value` lines, and scalars are text.
 *
 * An object at the top is always rendered as sections, the way the compiler
 * renders a file's exports and an anchor's properties; the whole compiled file
 * therefore renders the way `piton compile --renderer markdown` renders it.
 */
export function markdown(value: unknown, options: MarkdownOptions = {}): string {
  let level = options.level ?? 1;
  const out: string[] = [];
  if (options.title !== undefined) {
    out.push(heading(level, titleCase(options.title)), '\n');
    level += 1;
  }
  if (isObject(value) && referenceName(value) === undefined) {
    mdProperties(value, level, out);
  } else {
    mdValue(value, level - 1, out);
  }
  return out.join('').replace(/\n+$/, '') + '\n';
}

// ---------------------------------------------------------------------------

/** Every renderer, by name. */
export const renderers: Record<Renderer, (value: unknown) => string> = {
  json: (value) => json(value),
  yaml: (value) => yaml(value),
  markdown: (value) => markdown(value),
};

/** Renders a value through the named renderer. */
export function render(value: unknown, renderer: Renderer): string {
  const fn = renderers[renderer];
  if (!fn) {
    throw new Error(`\`${String(renderer)}\` is not a renderer. Use json, yaml or markdown.`);
  }
  return fn(value);
}

/** Whether a string names a renderer. */
export function isRenderer(name: unknown): name is Renderer {
  return name === 'json' || name === 'yaml' || name === 'markdown';
}
