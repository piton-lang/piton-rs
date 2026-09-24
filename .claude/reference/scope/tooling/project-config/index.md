# Project Config

## Description

If the compiler finds a piton.config.pi file in the current working directory, it will read that configuration file to configure a project. Realistically, this is how any Piton project will usually be used. Through a project you can configure the root, the entry point, where the output goes and which renderer it uses, frameworks, packages, and dependencies. By default output goes to ./dist as JSON. Frameworks decide where their own output goes, and a project with one only gets ./dist as well when it sets output or renderer.
Configuration anchors are provided by the @piton/config use/import which is bundled into the compiler:
```piton
use @piton/config
use @piton/belay

from @piton/belay import ClaudeCodeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi // Optional; defaults to index.pi in root
    output: ./dist // Optional; defaults to ./dist
    renderer: json // Optional; json, yaml, or markdown

    frameworks:
        - {BelayFrameworkConfig}

belay-config BelayFrameworkConfig:
    codeRoot: ./src/
    adapters:
        - {ClaudeCodeAdapter}
```

## Exports

The piton.config.pi file exports its main piton-config anchor, and the [Lsp](../cli/Lsp.md#lsp) needs to be aware that it's a project config, not just a regular Piton file.
