# vite-plugin-piton

Import `.pi` files in Vite.

```bash
npm install --save-dev vite-plugin-piton
```

The plugin runs the `piton` compiler, which must be on `PATH` — or name it
with the `binary` option. It is not a second implementation of the language:
a build that succeeds here is one `piton check` agrees with.

```js
// vite.config.js
import { defineConfig } from 'vite';
import piton from 'vite-plugin-piton';

export default defineConfig({
  plugins: [piton()],
});
```

```ts
import { SaveButton } from '../spec/app.pi';
import spec from '../spec/app.pi';

const button = SaveButton;
const cancel = spec.CancelButton;
```

## What an import gives you

Each export in the Piton file is a named export. The default export is the
whole file, the same as what `piton compile` gives you: an object with one key
per export (anything not exported, and abstract anchors, are left out).

An export whose name is not a JavaScript identifier — `my-button`, say — is
still exported under its own name, which you import with a string:

```js
import { 'my-button' as myButton } from '../spec/app.pi';
```

An export named `default` is only reachable as `spec.default`, since the name is
taken by the default export.

For types, add `"vite-plugin-piton/client"` to `compilerOptions.types`. Which
names a file exports depends on the file, so every import from a `.pi` module
is typed `any`.

## Options

| Option | Default | |
| --- | --- | --- |
| `renderer` | `'json'` | What a bare import renders through: `json`, `yaml` or `markdown`. |
| `binary` | `'piton'` | Path to the compiler. |
| `adapter` | | Deprecated name for `renderer`. |

With `json` the default export is the compiled object and each named export is
that export's value. With `yaml` or `markdown` the default export is the whole
file as text — exactly what `piton compile --renderer <renderer>` prints — and
each named export is that export rendered the same way.

## Renderers

A default renderer is configured once, but the renderers are also functions you
can import and use inline, on the whole file or on one export:

```ts
import { SaveButton } from '../spec/app.pi';
import { json, yaml, markdown } from 'vite-plugin-piton/renderers';

const asYaml = yaml(SaveButton);
const asDoc = markdown(SaveButton, { title: 'SaveButton' });
```

They are pure functions with no Node dependency, so they run in the browser as
well as in a build. They follow the compiler's rules:

- **JSON** is laid out the way `piton compile` lays it out.
- **YAML** leaves prose bare and quotes only what YAML would misread.
- **Markdown** turns property names into title-cased headings (`myProperty`
  becomes `My Property`), uses heading levels for the hierarchy and bold labels
  past level six, writes lists as Markdown lists and a dictionary holding only
  scalars and dictionaries as indented `key: value` lines in a fence, and
  writes scalars as text. `title` adds a heading for the value itself; `level`
  picks the starting heading level.

One limit: compiled JSON no longer says which objects were anchors, so the
Markdown renderer treats every nested object as a dictionary. A nested anchor
holding only scalars comes out fenced where the compiler would give it
headings. When the output has to match the compiler exactly, render the file
through it:

```js
import { renderFile, compileFile } from 'vite-plugin-piton';
// or: import { json, yaml, markdown } from 'vite-plugin-piton/files';

const spec = await compileFile('./spec/app.pi');
const guide = await renderFile('./spec/app.pi', 'markdown');
```

These run the compiler, so they work anywhere Node does — a build script, a
test, an Astro endpoint.

## Virtual modules

A bare import means the configured renderer. For another, import through a
virtual module:

```js
import guide from 'virtual:piton/markdown/spec/app.pi';
```

The path after the renderer is relative to the Vite root. The module has the
same shape as a bare import configured with that renderer.

## Hot reload and dependency tracking

Editing a `.pi` file reloads the modules that imported it. So does editing a
file it *imports*, which Vite cannot work out for itself: imports resolve
through Piton's module graph, and a package can keep a file somewhere the
importing text never names. The plugin compiles with
`piton compile --dependencies`, which reports every file the compilation read,
and watches all of them.

## Errors

A file that does not compile fails the build with the compiler's own
diagnostic, naming the file, line and cause. The plugin does not rewrite those
messages; it could only make them worse.

## Development

```bash
npm install
npm test     # builds, then runs the tests against the `piton` on PATH
```
