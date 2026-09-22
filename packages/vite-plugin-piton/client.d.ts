/**
 * Types for importing `.pi` files.
 *
 * Add `"vite-plugin-piton/client"` to `compilerOptions.types`, or reference
 * this file, so that `import spec from './app.pi'` type-checks.
 */

declare module '*.pi' {
  /** The file's anchors, by name. Present when the adapter is `json`. */
  export const anchors: Record<string, unknown>;
  /** The rendered output as text, whatever the adapter. */
  export const text: string;
  const module: { anchors: Record<string, unknown>; text: string };
  export default module;
}

declare module 'virtual:piton/*' {
  export const anchors: Record<string, unknown>;
  export const text: string;
  const module: { anchors: Record<string, unknown>; text: string } | string;
  export default module;
}
