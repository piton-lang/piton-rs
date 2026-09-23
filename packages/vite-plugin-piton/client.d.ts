/**
 * Types for importing `.pi` files.
 *
 * Add `"vite-plugin-piton/client"` to `compilerOptions.types`, or reference
 * this file, so that these type-check:
 *
 * ```ts
 * import { SaveButton } from '../spec/app.pi';
 * import spec from '../spec/app.pi';
 * ```
 *
 * The default export is the whole file (an object with one key per export,
 * as `piton compile` gives it, or the rendered text when the plugin's renderer
 * is `yaml` or `markdown`), and each export of the Piton file is a named
 * export. Which names a file exports depends on the file, so these are
 * shorthand declarations: every import from a `.pi` module is typed `any`.
 */

declare module '*.pi';

/** `virtual:piton/<json|yaml|markdown>/<file>`: one file through a chosen renderer. */
declare module 'virtual:piton/*';
