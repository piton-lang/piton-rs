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

```js
import spec from '../spec/app.pi';

const button = spec.anchors.SaveButton;
```

## What an import gives you

With the default `json` adapter, a `.pi` module exports `anchors` (the file's
anchors by name), `text` (the rendered JSON), and both together as the default
export. With `yaml` or `markdown` there is no structure to expose, so the
module is the rendered text.

For types, add `"vite-plugin-piton/client"` to `compilerOptions.types`.

## Options

| Option | Default | |
| --- | --- | --- |
| `adapter` | `'json'` | What a bare import renders through. |
| `binary` | `'piton'` | Path to the compiler. |

## Rendering a file more than one way

A bare import has to mean one thing, so it means the configured adapter. When
you need another, import through a virtual module:

```js
import guide from 'virtual:piton/markdown/spec/app.pi';
```

The path after the adapter is relative to the Vite root. Or call a renderer
directly, which works anywhere Node does — a build script, a test, an Astro
endpoint — and not only inside a Vite transform:

```js
import { json, markdown, yaml } from 'vite-plugin-piton/renderers';

const spec = await json('./spec/app.pi');
const guide = await markdown('./spec/app.pi');
```

## Hot reload

Editing a `.pi` file reloads the modules that imported it. So does editing a
file it *imports*, which Vite cannot work out for itself: imports resolve
through Piton's module graph, and a package can keep a file somewhere the
importing text never names. The compiler reports what it read, and the plugin
watches all of it.

## Errors

A file that does not compile fails the build with the compiler's own
diagnostic, naming the file, line and cause. The plugin does not rewrite those
messages; it could only make them worse.
