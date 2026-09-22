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
import spec from '../spec/app.pi';

const button = spec.anchors.SaveButton;
```

## Renderers

You should be able to configure the plugin with a default renderer (markdown, JSON, etc.) but you should also be able to import those renderers as functions and use them inline.
