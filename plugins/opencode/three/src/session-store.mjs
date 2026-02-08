import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const STORE_VERSION = 1;

function nowISO() {
  return new Date().toISOString();
}

function defaultStore() {
  return {
    version: STORE_VERSION,
    updatedAt: nowISO(),
    parents: {},
  };
}

function coerceStore(raw) {
  if (!raw || typeof raw !== 'object') return defaultStore();

  const parents = raw.parents && typeof raw.parents === 'object' ? raw.parents : {};
  return {
    version: STORE_VERSION,
    updatedAt: typeof raw.updatedAt === 'string' ? raw.updatedAt : nowISO(),
    parents,
  };
}

export function sessionStorePath(worktree) {
  return join(worktree, '.three', 'opencode-session-store.json');
}

export function readSessionStore(worktree) {
  const file = sessionStorePath(worktree);
  if (!existsSync(file)) return defaultStore();

  try {
    const text = readFileSync(file, 'utf8');
    const parsed = JSON.parse(text);
    return coerceStore(parsed);
  } catch {
    return defaultStore();
  }
}

export function writeSessionStore(worktree, store) {
  const file = sessionStorePath(worktree);
  const dir = dirname(file);
  mkdirSync(dir, { recursive: true });

  const normalized = coerceStore(store);
  normalized.updatedAt = nowISO();

  const tmp = `${file}.tmp-${process.pid}`;
  writeFileSync(tmp, `${JSON.stringify(normalized, null, 2)}\n`, 'utf8');
  renameSync(tmp, file);
}

function ensureParent(store, parentKey) {
  if (!store.parents[parentKey] || typeof store.parents[parentKey] !== 'object') {
    store.parents[parentKey] = {
      updatedAt: nowISO(),
      roles: {},
    };
  }

  if (!store.parents[parentKey].roles || typeof store.parents[parentKey].roles !== 'object') {
    store.parents[parentKey].roles = {};
  }

  return store.parents[parentKey];
}

export function getRoleSession(store, parentKey, role) {
  const parent = store?.parents?.[parentKey];
  if (!parent || typeof parent !== 'object') return null;

  const entry = parent.roles?.[role];
  if (!entry || typeof entry !== 'object') return null;

  const id = String(entry.sessionID ?? '').trim();
  return id || null;
}

export function setRoleSession(store, parentKey, role, sessionID, extra = {}) {
  const parent = ensureParent(store, parentKey);
  parent.updatedAt = nowISO();
  parent.roles[role] = {
    sessionID: String(sessionID),
    updatedAt: nowISO(),
    ...extra,
  };
}

export function removeRoleSession(store, parentKey, role) {
  const parent = store?.parents?.[parentKey];
  if (!parent || typeof parent !== 'object' || !parent.roles) return;
  delete parent.roles[role];
  parent.updatedAt = nowISO();
}
