// Tokenises a file with the TextMate grammar, the way VS Code does.
//
//     node scripts/tokenize.mjs <grammar.tmLanguage.json> <file>
//
// Prints one JSON array of `[line, startUtf16, endUtf16, scopes[]]`, with
// 0-based lines and the scope stack outermost first. This is the TextMate
// runner of the highlighter corpus (`crates/piton-syntax/tests/
// highlight_corpus.rs`), which reads the stack back through
// `editors/fixtures/scopes.toml`.
//
// It uses VS Code's own engine, `vscode-textmate` on `vscode-oniguruma`, rather
// than a reimplementation, because the point is to see what the editor sees:
// TextMate regexes are Oniguruma regexes, and other engines disagree with it on
// exactly the corners (lookbehind, `\p{L}`, `$` before a newline) a grammar
// leans on.
//
// The fences embed `source.json` and `source.yaml`. Those grammars ship with VS
// Code, not with this extension, so they load here as empty grammars: the
// embedding is exercised, and what JSON looks like is not this grammar's to
// test.

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const textmate = require("vscode-textmate");
const oniguruma = require("vscode-oniguruma");

const [grammarPath, filePath] = process.argv.slice(2);
if (!grammarPath || !filePath) {
  console.error("usage: tokenize.mjs <grammar.tmLanguage.json> <file>");
  process.exit(2);
}

const here = path.dirname(fileURLToPath(import.meta.url));
const wasm = readFileSync(
  path.join(here, "..", "node_modules", "vscode-oniguruma", "release", "onig.wasm"),
);
await oniguruma.loadWASM(wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength));

const registry = new textmate.Registry({
  onigLib: Promise.resolve({
    createOnigScanner: (patterns) => new oniguruma.OnigScanner(patterns),
    createOnigString: (text) => new oniguruma.OnigString(text),
  }),
  loadGrammar: async (scopeName) => {
    if (scopeName === "source.piton") {
      return textmate.parseRawGrammar(readFileSync(grammarPath, "utf8"), grammarPath);
    }
    // An embedded language: present, and empty.
    return { scopeName, patterns: [], repository: {} };
  },
});

const grammar = await registry.loadGrammar("source.piton");
const lines = readFileSync(filePath, "utf8").split("\n");
// A file ending in a newline has no last line after it.
if (lines.length > 0 && lines[lines.length - 1] === "") {
  lines.pop();
}

const out = [];
let state = textmate.INITIAL;
lines.forEach((line, index) => {
  const result = grammar.tokenizeLine(line, state);
  for (const token of result.tokens) {
    const end = Math.min(token.endIndex, line.length);
    if (end > token.startIndex) {
      out.push([index, token.startIndex, end, token.scopes]);
    }
  }
  state = result.ruleStack;
});
process.stdout.write(JSON.stringify(out));
