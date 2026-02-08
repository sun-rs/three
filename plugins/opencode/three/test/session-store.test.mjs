import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import {
  readSessionStore,
  writeSessionStore,
  getRoleSession,
  setRoleSession,
  sessionStorePath,
} from '../src/session-store.mjs';

function withTempWorktree(fn) {
  const dir = mkdtempSync(join(tmpdir(), 'three-opencode-store-'));
  try {
    fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('readSessionStore returns empty structure when file is missing', () => {
  withTempWorktree((worktree) => {
    const store = readSessionStore(worktree);
    assert.equal(typeof store, 'object');
    assert.deepEqual(store.parents, {});
    assert.equal(existsSync(sessionStorePath(worktree)), false);
  });
});

test('setRoleSession and getRoleSession are isolated by parent session', () => {
  const store = readSessionStore('/non-existent-worktree-for-in-memory-use');

  setRoleSession(store, 'parent-a', 'oracle', 'child-1');
  setRoleSession(store, 'parent-b', 'oracle', 'child-2');

  assert.equal(getRoleSession(store, 'parent-a', 'oracle'), 'child-1');
  assert.equal(getRoleSession(store, 'parent-b', 'oracle'), 'child-2');
  assert.equal(getRoleSession(store, 'parent-a', 'builder'), null);
});

test('writeSessionStore persists and readSessionStore restores role sessions', () => {
  withTempWorktree((worktree) => {
    const store = readSessionStore(worktree);
    setRoleSession(store, 'parent-x', 'researcher', 'child-r-1');

    writeSessionStore(worktree, store);

    const restored = readSessionStore(worktree);
    assert.equal(getRoleSession(restored, 'parent-x', 'researcher'), 'child-r-1');
  });
});
