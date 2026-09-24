/**
 * The plugin tests run the real `piton` binary, because the point of the
 * plugin is that it defers to the compiler rather than reimplementing it. A
 * test against a stubbed compiler would pass while the pair disagreed.
 *
 * Run with `npm test` from the package directory (it builds first), with
 * `piton` on PATH.
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

import piton, { moduleFor } from '../dist/index.js';

async function project() {
  const root = await mkdtemp(path.join(tmpdir(), 'piton-vite-'));
  await mkdir(path.join(root, 'spec'), { recursive: true });
  await writeFile(
    path.join(root, 'piton.config.pi'),
    'export piton-config P:\n    root: .\n    entry: ./spec/app.pi\n',
  );
  await writeFile(
    path.join(root, 'spec', 'base.pi'),
    'export anchor Base:\n    kind: base\n',
  );
  await writeFile(
    path.join(root, 'spec', 'app.pi'),
    'from ./base import Base\n\n' +
      'export anchor SaveButton:\n    color: blue\n    uses: {Base}\n\n' +
      'export anchor CancelButton:\n    color: red\n',
  );
  return root;
}

/** The subset of the plugin context the hooks actually use. */
function context(watched = []) {
  return {
    addWatchFile(file) {
      watched.push(file);
    },
  };
}

/** Evaluates generated module code the way a bundler would. */
function evaluate(code) {
  return import('data:text/javascript;charset=utf-8,' + encodeURIComponent(code));
}

test('each export is a named export and the default is the whole file', async () => {
  const root = await project();
  const plugin = piton();
  plugin.configResolved({ root });

  const code = await plugin.load.call(context(), path.join(root, 'spec', 'app.pi'));
  const module = await evaluate(code);

  // The specification's own example:
  //   import { SaveButton } from '../spec/app.pi';
  //   import spec from '../spec/app.pi';
  //   spec.CancelButton
  assert.equal(module.SaveButton.color, 'blue');
  assert.equal(module.SaveButton.uses.kind, 'base');
  assert.equal(module.default.CancelButton.color, 'red');
  assert.equal(module.default.SaveButton, module.SaveButton);
  assert.equal(module.anchors, undefined);
});

test('an export whose name is not an identifier is exported under its own name', async () => {
  const code = moduleFor(
    { exports: { 'my-button': { color: 'green' }, class: 1, default: 2, plain: 3 } },
    'json',
  );
  const module = await evaluate(code);
  assert.deepEqual(module['my-button'], { color: 'green' });
  assert.equal(module.class, 1);
  assert.equal(module.plain, 3);
  // `default` is the whole file; the export of that name lives inside it.
  assert.equal(module.default.default, 2);
});

test('an imported file is watched even though nothing names it', async () => {
  const root = await project();
  const plugin = piton();
  plugin.configResolved({ root });

  const watched = [];
  await plugin.load.call(context(watched), path.join(root, 'spec', 'app.pi'));

  // `base.pi` is reached through an import inside the Piton source, which Vite
  // cannot see for itself.
  assert.ok(watched.some((file) => file.endsWith('base.pi')), watched.join(', '));
});

test('editing a dependency reloads the file that imported it', async () => {
  const root = await project();
  const plugin = piton();
  plugin.configResolved({ root });

  const entry = path.join(root, 'spec', 'app.pi');
  await plugin.load.call(context(), entry);

  const module = { id: entry };
  const server = { moduleGraph: { getModuleById: (id) => (id === entry ? module : null) } };
  const reload = plugin.handleHotUpdate({
    file: path.join(root, 'spec', 'base.pi'),
    server,
    modules: [],
  });
  assert.deepEqual(reload, [module]);
});

test('a relative import resolves against the file that wrote it', async () => {
  const root = await project();
  const plugin = piton();
  plugin.configResolved({ root });

  const importer = path.join(root, 'src', 'page.js');
  const resolved = await plugin.resolveId('../spec/app.pi', importer);
  assert.equal(resolved, path.join(root, 'spec', 'app.pi'));
});

test('a virtual module renders a file a second way', async () => {
  const root = await project();
  const plugin = piton();
  plugin.configResolved({ root });

  const id = await plugin.resolveId('virtual:piton/markdown/spec/app.pi', undefined);
  const module = await evaluate(await plugin.load.call(context(), id));

  // The default export is what `piton compile --renderer markdown` prints.
  assert.equal(typeof module.default, 'string');
  assert.match(module.default, /^# Save Button$/m);
  // Each export is rendered the same way.
  assert.match(module.CancelButton, /^# Cancel Button\n\n## Color\n\nred\n$/);
});

test('the renderer option picks what a bare import renders through', async () => {
  const root = await project();
  const entry = path.join(root, 'spec', 'app.pi');

  for (const options of [{ renderer: 'yaml' }]) {
    const plugin = piton(options);
    plugin.configResolved({ root });
    const module = await evaluate(await plugin.load.call(context(), entry));
    assert.match(module.default, /^SaveButton:$/m);
    assert.equal(module.CancelButton, 'color: red\n');
  }
});

test('an unknown renderer is refused', () => {
  assert.throws(() => piton({ renderer: 'html' }), /not a renderer/);
});

test('a virtual module with no renderer says so', async () => {
  const root = await project();
  const plugin = piton();
  plugin.configResolved({ root });

  const id = await plugin.resolveId('virtual:piton/spec/app.pi', undefined);
  await assert.rejects(() => plugin.load.call(context(), id), /names no renderer/);
});

test('a compiler error carries the compiler diagnostic', async () => {
  const root = await project();
  await writeFile(
    path.join(root, 'spec', 'app.pi'),
    'export anchor Broken:\n    x: {Missing}\n',
  );
  const plugin = piton();
  plugin.configResolved({ root });

  await assert.rejects(
    () => plugin.load.call(context(), path.join(root, 'spec', 'app.pi')),
    /is not in scope/,
  );
});

test('the value renderers agree with the compiler on a whole file', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'piton-vite-md-'));
  const file = path.join(root, 'doc.pi');
  await writeFile(
    file,
    'export anchor OrderOfPrecedence:\n' +
      '    shortDescription: Operators bind in this order\n' +
      '    examples:\n' +
      '        sourceName: myProperty\n' +
      '        renderedTitle: My Property\n' +
      '    output:\n' +
      '        description: what it does\n' +
      '        requirements:\n' +
      '            - one\n' +
      '            - two\n',
  );
  const { json, markdown, yaml } = await import('../dist/renderers.js');
  const { compileFile, renderFile } = await import('../dist/index.js');
  const value = await compileFile(file, { cwd: root });
  assert.equal(markdown(value), await renderFile(file, 'markdown', { cwd: root }));
  assert.equal(yaml(value), await renderFile(file, 'yaml', { cwd: root }));
  assert.equal(json(value), await renderFile(file, 'json', { cwd: root }));
});

test('files can be rendered through the compiler by path', async () => {
  const root = await project();
  const files = await import('../dist/files.js');
  const spec = await files.json(path.join(root, 'spec', 'app.pi'), { cwd: root });
  assert.equal(spec.SaveButton.color, 'blue');
  const text = await files.yaml(path.join(root, 'spec', 'app.pi'), { cwd: root });
  const direct = execFileSync('piton', ['compile', '--renderer', 'yaml', path.join(root, 'spec', 'app.pi')], {
    cwd: root,
    encoding: 'utf8',
  });
  assert.equal(text, direct);
});
