# Vite Plugin

## Features

- imports .pi files
- HMR
- dependency tracking
- virtual modules

## Description

Should be a Vite plugin that allows loading a Piton file and reading properties from it.
For example
```typescript
import { SaveButton, pi } from '../spec/app.pi';
import spec from '../spec/app.pi';

const button = SaveButton;
const cancel = spec.CancelButton;
```
Each export in the Piton file is a named export. The default export is the whole file, same as what piton compile gives you.

## Renderers

You should be able to configure the plugin with a default renderer (markdown, JSON, etc.) but you should also be able to import those renderers as functions and use them inline.
