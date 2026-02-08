export function normalizeAgentName(agent) {
  return String(agent ?? '').trim();
}

function normalizeMode(mode) {
  const value = String(mode ?? '').trim().toLowerCase();
  if (value === 'primary' || value === 'subagent' || value === 'all') {
    return value;
  }
  return 'all';
}

export function parseModelRef(ref) {
  const raw = String(ref ?? '').trim();
  if (!raw) return null;

  const atIdx = raw.lastIndexOf('@');
  const hasVariant = atIdx > 0 && atIdx < raw.length - 1;
  const base = hasVariant ? raw.slice(0, atIdx) : raw;
  const variant = hasVariant ? raw.slice(atIdx + 1).trim() : undefined;

  const slash = base.indexOf('/');
  if (slash <= 0 || slash >= base.length - 1) return null;

  const providerID = base.slice(0, slash).trim();
  const modelID = base.slice(slash + 1).trim();
  if (!providerID || !modelID) return null;

  return {
    providerID,
    modelID,
    variant: variant || undefined,
  };
}

export function buildAgentCatalog(agents) {
  const all = [];
  const callable = [];
  const primary = [];
  const byName = new Map();

  for (const item of agents ?? []) {
    const name = normalizeAgentName(item?.name);
    if (!name) continue;

    const mode = normalizeMode(item?.mode);
    const entry = {
      name,
      mode,
      model: item?.model ?? null,
    };

    all.push(entry);
    byName.set(name.toLowerCase(), entry);

    if (mode === 'primary') {
      primary.push(entry);
    } else {
      callable.push(entry);
    }
  }

  return {
    all,
    callable,
    primary,
    byName,
  };
}

export function resolveRequestedAgent(requestedAgent, catalog) {
  const requested = normalizeAgentName(requestedAgent);
  if (!requested) {
    return { agent: null, error: 'agent is required' };
  }

  const match = catalog.byName.get(requested.toLowerCase());
  if (!match) {
    const available = catalog.callable.map((item) => item.name).sort().join(', ');
    return {
      agent: null,
      error: `unknown agent '${requested}'. callable agents: ${available || '(none)'}`,
    };
  }

  if (match.mode === 'primary') {
    return {
      agent: null,
      error: `cannot call primary agent '${match.name}' as a sub-session participant`,
    };
  }

  return {
    agent: match,
    error: null,
  };
}
