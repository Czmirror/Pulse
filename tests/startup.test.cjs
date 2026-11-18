// 配信HTMLの起動処理を実行し、失敗時の復旧と正常起動を検証する。
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const { test } = require('node:test');
const vm = require('node:vm');

const html = readFileSync(resolve(__dirname, '../web/index.html'), 'utf8');
const scripts = [...html.matchAll(/<script type="module">([\s\S]*?)<\/script>/g)];
assert.equal(scripts.length, 1);

async function start(mode) {
  const state = { errors: [], classes: [], imports: [], initialized: 0, reloads: 0 };
  const message = { textContent: '' };
  let retry;
  const button = {
    addEventListener(event, handler) {
      assert.equal(event, 'click');
      retry = handler;
    },
    focus() { state.focused = true; },
  };
  const context = vm.createContext({
    navigator: mode === 'webgpu' ? { gpu: {} } : {},
    document: {
      getElementById: id => id === 'error-message' ? message : button,
      createElement: () => ({ getContext: () => !['unsupported', 'webgpu'].includes(mode) }),
      body: { classList: { add: value => state.classes.push(value) } },
    },
    window: { location: { reload: () => state.reloads++ } },
    console: { error: value => state.errors.push(value) },
  });
  const module = new vm.SourceTextModule(scripts[0][1], {
    context,
    importModuleDynamically: async specifier => {
      state.imports.push(specifier);
      if (mode === 'missing-js') throw new Error('Network failure');
      const game = new vm.SyntheticModule(['default'], function () {
        this.setExport('default', async () => {
          state.initialized++;
          if (mode === 'missing-wasm') throw new Error('WASM failure');
          if (mode === 'winit') throw new Error("Using exceptions for control flow, don't mind me. This isn't actually an error!");
        });
      }, { context });
      await game.link(() => assert.fail('Unexpected dependency'));
      await game.evaluate();
      return game;
    },
  });
  await module.link(() => assert.fail('Startup must catch import failures'));
  await module.evaluate();
  // 起動処理はトップレベルでawaitされないため、Promiseの完了を待つ。
  await new Promise(resolve => setImmediate(resolve));
  return { state, message, retry };
}

for (const mode of ['webgl', 'webgpu', 'winit']) {
  test(mode + ' starts without an error panel', async () => {
    const { state } = await start(mode);
    assert.deepEqual(state.imports, ['./pulse.js']);
    assert.equal(state.initialized, 1);
    assert.deepEqual(state.classes, []);
    assert.deepEqual(state.errors, []);
    assert.equal(state.focused, undefined);
  });
}

for (const mode of ['missing-js', 'missing-wasm']) {
  test(mode + ' offers focused recovery and reload', async () => {
    const { state, message, retry } = await start(mode);
    assert.deepEqual(state.classes, ['error']);
    assert.match(message.textContent, /failed to load/);
    assert.equal(state.focused, true);
    assert.equal(state.errors.length, 1);
    assert.ok(!message.textContent.includes('failure'));
    retry();
    assert.equal(state.reloads, 1);
  });
}

test('unsupported browsers do not load the game', async () => {
  const { state, message } = await start('unsupported');
  assert.deepEqual(state.imports, []);
  assert.deepEqual(state.classes, ['error']);
  assert.match(message.textContent, /WebGL2 or WebGPU/);
  assert.equal(state.focused, true);
});
