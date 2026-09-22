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
import spec from '../spec/app.pi';
const button = spec.anchors.SaveButton;
---
<button style={`background: ${button.color}`}>Save</button>
```

Astro builds on Vite, so this adds [`vite-plugin-piton`](../vite-plugin-piton)
to Astro's Vite configuration and gets out of the way. Hot reload, dependency
tracking and virtual modules all work as they do there, and the options are the
same.

Type declarations for `.pi` imports are injected into `.astro/`, so
`import spec from './app.pi'` type-checks without any further setup.

The `piton` compiler must be on `PATH`, or named with the `binary` option.
