import test from 'node:test';
import assert from 'node:assert/strict';

import {
  computeParentStoreKey,
  clampRoundCount,
  buildRoundPrompt,
  summarizeRound,
  resolveRound1ForceNew,
  buildRoundContext,
  mergeRoundContext,
  resolveRoundContextLimits,
  resolveRoundStageTimeoutSecs,
  resolveRoundStageMinSuccesses,
} from '../src/orchestrator.mjs';

test('computeParentStoreKey isolates by conversation_id', () => {
  assert.equal(computeParentStoreKey('session-a', undefined), 'session-a');
  assert.equal(computeParentStoreKey('session-a', 'conv-1'), 'session-a::conv-1');
});


test('clampRoundCount no longer caps at 3', () => {
  assert.equal(clampRoundCount(undefined), 2);
  assert.equal(clampRoundCount(1), 1);
  assert.equal(clampRoundCount(5), 5);
  assert.equal(clampRoundCount(9.8), 9);
});

test('resolveRoundStageTimeoutSecs applies defaults and clamping', () => {
  assert.equal(resolveRoundStageTimeoutSecs(undefined), 90);
  assert.equal(resolveRoundStageTimeoutSecs(-1), 90);
  assert.equal(resolveRoundStageTimeoutSecs(3), 15);
  assert.equal(resolveRoundStageTimeoutSecs(120), 120);
  assert.equal(resolveRoundStageTimeoutSecs(9999), 600);
});

test('resolveRoundStageMinSuccesses falls back and caps by participant count', () => {
  assert.equal(resolveRoundStageMinSuccesses(undefined, 5), 3);
  assert.equal(resolveRoundStageMinSuccesses(undefined, 2), 2);
  assert.equal(resolveRoundStageMinSuccesses(1, 5), 1);
  assert.equal(resolveRoundStageMinSuccesses(10, 4), 4);
});

test('resolveRoundContextLimits supports level presets and overrides', () => {
  const preset = resolveRoundContextLimits({ level: 'compact' });
  assert.equal(preset.level, 'compact');
  assert.ok(preset.perAgentChars >= 600);
  assert.ok(preset.totalChars >= preset.perAgentChars);

  const custom = resolveRoundContextLimits({
    level: 'balanced',
    perAgentChars: 5000,
    totalChars: 18000,
  });
  assert.equal(custom.level, 'balanced');
  assert.equal(custom.perAgentChars, 5000);
  assert.equal(custom.totalChars, 18000);
});

test('buildRoundPrompt includes substantial previous viewpoints for round2+', () => {
  const p1 = buildRoundPrompt({
    round: 1,
    totalRounds: 3,
    agent: 'oracle',
    topic: 'decide architecture',
  });
  assert.match(p1, /ROUND 1\/3/);
  assert.match(p1, /TOPIC:/);

  const p2 = buildRoundPrompt({
    round: 2,
    totalRounds: 3,
    agent: 'oracle',
    topic: 'decide architecture',
    previousContext: '[metis]\npoint a\n\n[momus]\npoint b',
  });
  assert.match(p2, /ROUND 2\/3/);
  assert.match(p2, /Previous round viewpoints/);
  assert.match(p2, /named peers/);
  assert.match(p2, /\[metis\]/);
  assert.match(p2, /\[momus\]/);
});

test('buildRoundPrompt supports anonymous peer references', () => {
  const prompt = buildRoundPrompt({
    round: 2,
    totalRounds: 3,
    agent: 'oracle',
    topic: 'decide architecture',
    previousContext: '[Response A]\npoint a',
    anonymousViewpoints: true,
  });

  assert.match(prompt, /anonymized substantial excerpts/);
  assert.match(prompt, /response labels/);
});

test('buildRoundContext carries multi-agent excerpts instead of one-line summaries', () => {
  const context = buildRoundContext([
    {
      agent: 'oracle',
      success: true,
      text: [
        'Position: keep hot path stable.',
        'Reason 1: avoid write-side contention.',
        'Reason 2: add observability counters.',
      ].join('\n'),
    },
    {
      agent: 'metis',
      success: true,
      text: [
        'Position: optimize only with data.',
        'Risk: over-optimization before metrics.',
      ].join('\n'),
    },
    {
      agent: 'momus',
      success: false,
      error: 'timeout',
      text: '',
    },
  ], {
    level: 'balanced',
    perAgentChars: 1200,
    totalChars: 4000,
  });

  assert.match(context.text, /\[oracle\]/);
  assert.match(context.text, /Reason 2: add observability counters\./);
  assert.match(context.text, /\[metis\]/);
  assert.match(context.text, /Position: optimize only with data\./);
  assert.match(context.text, /Errors from previous round/);
  assert.equal(context.stats.success_count, 2);
  assert.equal(context.stats.failed_count, 1);
  assert.equal(context.stats.anonymous_viewpoints, false);
});

test('buildRoundContext anonymous mode hides agent names in carryover text', () => {
  const context = buildRoundContext([
    {
      agent: 'oracle',
      success: true,
      text: 'oracle details',
    },
    {
      agent: 'metis',
      success: true,
      text: 'metis details',
    },
  ], {
    level: 'compact',
    perAgentChars: 800,
    totalChars: 2000,
    anonymousViewpoints: true,
  });

  assert.match(context.text, /\[Response A\]/);
  assert.match(context.text, /\[Response B\]/);
  assert.equal(context.stats.anonymous_viewpoints, true);
  assert.equal(Array.isArray(context.stats.label_map), true);
  assert.equal(context.stats.label_map.length, 2);
});


test('mergeRoundContext accumulates rounds up to configured limit', () => {
  const r1 = '=== Round 1 viewpoints ===\n[oracle]\nfirst round details';
  const r2 = '=== Round 2 viewpoints ===\n[metis]\nsecond round details';

  const merged = mergeRoundContext(r1, r2, {
    level: 'balanced',
    totalChars: 2000,
    perAgentChars: 1000,
  });

  assert.match(merged.text, /Round 1 viewpoints/);
  assert.match(merged.text, /Round 2 viewpoints/);
  assert.equal(merged.truncated, false);
});


test('mergeRoundContext truncation keeps latest rounds when over limit', () => {
  const oldChunk = `=== Round 1 viewpoints ===\n${'old '.repeat(400)}`;
  const newChunk = `=== Round 4 viewpoints ===\n${'new '.repeat(120)}`;

  const merged = mergeRoundContext(oldChunk, newChunk, {
    level: 'compact',
    totalChars: 1500,
    perAgentChars: 1000,
  });

  assert.equal(merged.truncated, true);
  assert.match(merged.text, /Round 4 viewpoints/);
});

test('summarizeRound reports consensus and disagreement markers', () => {
  const summary = summarizeRound([
    { agent: 'oracle', success: true, text: 'Use layered approach for stability.' },
    { agent: 'critic', success: true, text: 'Layered approach risks split-brain if boundaries unclear.' },
    { agent: 'builder', success: false, error: 'timeout' },
  ]);

  assert.match(summary, /Participants: 3/);
  assert.match(summary, /Success: 2/);
  assert.match(summary, /Failed: 1/);
  assert.match(summary, /oracle/);
  assert.match(summary, /critic/);
});


test('resolveRound1ForceNew obeys participant override and global fallback', () => {
  assert.equal(resolveRound1ForceNew(undefined, true), true);
  assert.equal(resolveRound1ForceNew(undefined, false), false);
  assert.equal(resolveRound1ForceNew(false, true), false);
  assert.equal(resolveRound1ForceNew(true, false), true);
});
