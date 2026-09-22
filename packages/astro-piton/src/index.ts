/**
 * An Astro integration for Piton.
 *
 * Astro builds on Vite, so this adds the Vite plugin to Astro's own Vite
 * configuration and gets out of the way. Everything the plugin does -- hot
 * reload, dependency tracking, virtual modules -- follows from that, and works
 * the same in an Astro project as in a bare Vite one.
 *
 * The type declarations for `.pi` imports are added too, since Astro projects
 * are type-checked by default and `import spec from './app.pi'` is otherwise
 * an error in the editor.
 */

import piton, { type PitonPluginOptions } from 'vite-plugin-piton';

export interface AstroPitonOptions extends PitonPluginOptions {}

/** Piton support for an Astro project. */
export default function astroPiton(options: AstroPitonOptions = {}) {
  return {
    name: 'astro-piton',
    hooks: {
      'astro:config:setup': ({
        updateConfig,
      }: {
        updateConfig: (config: Record<string, unknown>) => void;
      }) => {
        updateConfig({ vite: { plugins: [piton(options)] } });
      },

      'astro:config:done': ({
        injectTypes,
      }: {
        injectTypes?: (types: { filename: string; content: string }) => void;
      }) => {
        // Older Astro versions have no `injectTypes`; the plugin still works,
        // the editor just will not know the shape of a `.pi` import.
        injectTypes?.({
          filename: 'piton.d.ts',
          content: [
            "declare module '*.pi' {",
            '  export const anchors: Record<string, unknown>;',
            '  export const text: string;',
            '  const module: { anchors: Record<string, unknown>; text: string };',
            '  export default module;',
            '}',
            '',
          ].join('\n'),
        });
      },
    },
  };
}
