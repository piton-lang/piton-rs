/**
 * These run the real `piton` binary, because the point of the plugin is that
 * it defers to the compiler rather than reimplementing it. A test against a
 * stubbed compiler would pass while the pair disagreed.
 *
 * Run with `node --test test/` from the package directory, with `piton` on
 * PATH.
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

import piton from '../dist/index.js';

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
    'from ./base import Base\n\nexport anchor SaveButton:\n    color: blue\n    uses: {Base}\n',
  );
  return root;
}

/** The subset of the plugin context the hooks actually use. */
function context(watched) {
  return {
    addWatchFile(file) {
      watched.push(file);
    },
  };
}

test('a .pi import becomes its anchors', async () => {
  const root = await project();
  const plugin = piton();
  plugin.configResolved({ root });

  const watched = [];
  const file = path.join(root, 'spec', 'app.pi');
  const code = await plugin.load.call(context(watched), file);

  assert.match(code, /export const anchors =/);
  // The specification's own example: spec.anchors.SaveButton.
  const anchors = JSON.parse(code.match(/export const anchors = (.*);\n/)[1]);
  assert.equal(anchors.SaveButton.color, 'blue');
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
  await plugin.load.call(context([]), entry);

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
  const code = await plugin.load.call(context([]), id);
  assert.match(code, /export default text/);
  assert.doesNotMatch(code, /export const anchors/);
});

test('a virtual module with no adapter says so', async () => {
  const root = await project();
  const plugin = piton();
  plugin.configResolved({ root });

  const id = await plugin.resolveId('virtual:piton/spec/app.pi', undefined);
  await assert.rejects(() => plugin.load.call(context([]), id), /names no adapter/);
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
    () => plugin.load.call(context([]), path.join(root, 'spec', 'app.pi')),
    /is not in scope/,
  );
});
