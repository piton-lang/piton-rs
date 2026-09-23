# JavaScript packages

Piton's compiler, language server and editor support are Rust. These two
packages are not, because a Vite plugin has to be JavaScript to be loaded by
Vite.

Neither reimplements the language. Both run the `piton` binary and read what it
reports, so there is one compiler and one set of diagnostics. The one thing
written in TypeScript is the set of renderers (JSON, YAML, Markdown) that turn
an already compiled value into text inline, following the compiler's own
rendering rules.

- [`vite-plugin-piton`](vite-plugin-piton) — import `.pi` files in Vite.
- [`astro-piton`](astro-piton) — the same, for Astro.
