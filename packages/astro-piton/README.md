# astro-piton

Import `.pi` files in an Astro project.

```bash
npm install --save-dev astro-piton
```

```js
// astro.config.mjs
import { defineConfig } from 'astro/config';
import piton from 'astro-piton';

export default defineConfig({
  integrations: [piton()],
});
```

```astro
---
import { SaveButton } from '../spec/app.pi';
import spec from '../spec/app.pi';
import { markdown } from 'astro-piton';

const cancel = spec.CancelButton;
---
<button style={`background: ${SaveButton.color}`}>Save</button>
<button style={`background: ${cancel.color}`}>Cancel</button>
<pre>{markdown(SaveButton, { title: 'SaveButton' })}</pre>
```

Astro builds on Vite, so this adds [`vite-plugin-piton`](../vite-plugin-piton)
to Astro's Vite configuration and gets out of the way. Named exports, the
default export, hot reload, dependency tracking, virtual modules and the
renderers all work as they do there, and the options (`renderer`, `binary`)
are the same. The renderers are re-exported from `astro-piton` for
convenience.

Type declarations for `.pi` imports are injected into `.astro/`, so
`import { SaveButton } from './app.pi'` type-checks without any further setup.
Every import from a `.pi` module is typed `any`, because which names a file
exports depends on the file.

The `piton` compiler must be on `PATH`, or named with the `binary` option.
