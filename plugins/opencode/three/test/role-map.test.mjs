import test from 'node:test';
import assert from 'node:assert/strict';

import {
  buildAgentCatalog,
  normalizeAgentName,
  parseModelRef,
  resolveRequestedAgent,
} from '../src/role-map.mjs';

test('normalizeAgentName trims input', () => {
  assert.equal(normalizeAgentName(' oracle '), 'oracle');
});

test('buildAgentCatalog separates primary and callable agents', () => {
  const catalog = buildAgentCatalog([
    { name: 'sisyphus', mode: 'primary' },
    { name: 'oracle', mode: 'subagent' },
    { name: 'librarian', mode: 'all' },
  ]);

  assert.equal(catalog.primary.length, 1);
  assert.equal(catalog.primary[0].name, 'sisyphus');
  assert.equal(catalog.callable.length, 2);
  assert.equal(catalog.byName.get('oracle').name, 'oracle');
});

test('resolveRequestedAgent rejects primary and unknown agents', () => {
  const catalog = buildAgentCatalog([
    { name: 'sisyphus', mode: 'primary' },
    { name: 'oracle', mode: 'subagent' },
  ]);

  const unknown = resolveRequestedAgent('builder', catalog);
  assert.equal(unknown.agent, null);
  assert.match(String(unknown.error), /unknown agent/i);

  const primary = resolveRequestedAgent('sisyphus', catalog);
  assert.equal(primary.agent, null);
  assert.match(String(primary.error), /primary agent/i);

  const ok = resolveRequestedAgent('oracle', catalog);
  assert.equal(ok.error, null);
  assert.equal(ok.agent?.name, 'oracle');
});

test('parseModelRef supports provider/model@variant and trims input', () => {
  assert.deepEqual(parseModelRef(' openai/gpt-5.2@xhigh '), {
    providerID: 'openai',
    modelID: 'gpt-5.2',
    variant: 'xhigh',
  });

  assert.deepEqual(parseModelRef('anthropic/claude-opus-4-5'), {
    providerID: 'anthropic',
    modelID: 'claude-opus-4-5',
    variant: undefined,
  });

  assert.equal(parseModelRef(''), null);
  assert.equal(parseModelRef('invalid'), null);
});
