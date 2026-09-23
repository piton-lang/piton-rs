/**
 * The value renderers are pure functions, so these need no compiler. The
 * expectations are the compiler's own renderer tests
 * (crates/piton-emit/src/{markdown,yaml,json}.rs), so the two stay in step.
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';

import { json, markdown, render, titleCase, yaml } from '../dist/renderers.js';

test('property names become word-separated titles', () => {
  assert.equal(titleCase('myProperty'), 'My Property');
  assert.equal(titleCase('orderOfPrecedence'), 'Order Of Precedence');
  assert.equal(titleCase('whatIsAType'), 'What Is AType');
  assert.equal(titleCase('build-tooling'), 'Build Tooling');
  assert.equal(markdown({ orderOfPrecedence: 'text' }, { level: 2 }), '## Order Of Precedence\n\ntext\n');
});

test('pure dictionaries are fenced', () => {
  const rendered = markdown(
    { examples: { sourceName: 'myProperty', renderedTitle: 'My Property' } },
    { level: 2 },
  );
  assert.equal(
    rendered,
    '## Examples\n\n```\nsourceName: myProperty\nrenderedTitle: My Property\n```\n',
  );
});

test('dictionaries holding lists become headings', () => {
  const rendered = markdown(
    { output: { description: 'what it does', requirements: ['one'] } },
    { level: 2 },
  );
  assert.equal(
    rendered,
    '## Output\n\n### Description\n\nwhat it does\n\n### Requirements\n\n- one\n',
  );
});

test('lists of dictionaries indent their keys', () => {
  const rendered = markdown(
    { operators: [{ description: 'Adds two numbers together', symbol: '+' }] },
    { level: 2 },
  );
  assert.equal(rendered, '## Operators\n\n- description: Adds two numbers together\n  symbol: +\n');
});

test('beyond six levels titles become bold labels', () => {
  let value = { deep: ['x'] };
  for (let i = 0; i < 6; i++) {
    value = { level: value, list: ['keeps it structural'] };
  }
  const rendered = markdown(value);
  assert.match(rendered, /^###### Level$/m);
  assert.match(rendered, /^\*\*Deep\*\*$/m);
});

test('multiline values stay inside their entry, and primitives render as text', () => {
  assert.equal(markdown({ box: { note: 'first\nsecond' } }), '# Box\n\n```\nnote: first\n  second\n```\n');
  assert.equal(
    markdown({ box: { booleanValue: false, numericValue: 42, missing: null } }),
    '# Box\n\n```\nbooleanValue: false\nnumericValue: 42\nmissing: null\n```\n',
  );
});

test('a title renders a value as its own document', () => {
  assert.equal(
    markdown({ color: 'blue' }, { title: 'SaveButton' }),
    '# Save Button\n\n## Color\n\nblue\n',
  );
});

test('a reference renders as the name it points at', () => {
  assert.equal(markdown({ base: { $ref: '/x/base.pi#Base' } }), '# Base\n\nBase\n');
  assert.equal(yaml({ base: { $ref: '/x/base.pi#Base' } }), 'base: "!ref /x/base.pi#Base"\n');
});

test('YAML leaves prose bare and quotes what would be misread', () => {
  assert.equal(yaml({ description: 'Adds two numbers' }), 'description: Adds two numbers\n');
  assert.equal(
    yaml({ a: 'true', b: '42', c: '+', d: 'a: b', e: true }),
    'a: "true"\nb: "42"\nc: +\nd: "a: b"\ne: true\n',
  );
  assert.equal(
    yaml({ list: ['one', { key: 'v', other: 2 }], empty: [], none: {} }),
    'list:\n- one\n- key: v\n  other: 2\nempty: []\nnone: {}\n',
  );
});

test('JSON is laid out the way the compiler lays it out', () => {
  assert.equal(
    json({ a: [1, 'two'], b: {}, c: { $ref: 'f.pi#C' } }),
    '{\n  "a": [\n    1,\n    "two"\n  ],\n  "b": {},\n  "c": {"$ref": "f.pi#C"}\n}\n',
  );
  assert.equal(render({ a: 1 }, 'json'), json({ a: 1 }));
  assert.throws(() => render({}, 'html'), /not a renderer/);
});
