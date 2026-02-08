import {
  parseModelRef,
  buildAgentCatalog,
  resolveRequestedAgent,
  normalizeAgentName,
} from './role-map.mjs';
import {
  readSessionStore,
  writeSessionStore,
  getRoleSession,
  setRoleSession,
  removeRoleSession,
  sessionStorePath,
} from './session-store.mjs';
import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

export const DEFAULT_ROUNDTABLE_CONTEXT = Object.freeze({
  level: 'rich',
  perAgentChars: 6000,
  totalChars: 60000,
});

const ROUNDTABLE_CONTEXT_PRESETS = Object.freeze({
  compact: Object.freeze({ perAgentChars: 1400, totalChars: 12000 }),
  balanced: Object.freeze({ perAgentChars: 3200, totalChars: 32000 }),
  rich: Object.freeze({ perAgentChars: 6000, totalChars: 60000 }),
});

const DEFAULT_ROUND_STAGE_TIMEOUT_SECS = 90;
const MIN_ROUND_STAGE_TIMEOUT_SECS = 15;
const MAX_ROUND_STAGE_TIMEOUT_SECS = 600;
const ROUND_STAGE_TIMEOUT_EXTENSION_CAP_MS = 30000;

function unwrapData(result) {
  return result && typeof result === 'object' && 'data' in result ? result.data : result;
}

function unwrapError(result) {
  return result && typeof result === 'object' && 'error' in result ? result.error : null;
}

function formatModelRef(model) {
  if (!model) return null;
  return model.variant
    ? `${model.providerID}/${model.modelID}@${model.variant}`
    : `${model.providerID}/${model.modelID}`;
}

function parsePositiveInt(value, fallback, minValue) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) return fallback;
  return Math.max(Math.floor(parsed), minValue);
}

function normalizeContextLevel(level) {
  const cleaned = String(level ?? '').trim().toLowerCase();
  if (cleaned && ROUNDTABLE_CONTEXT_PRESETS[cleaned]) {
    return cleaned;
  }
  return DEFAULT_ROUNDTABLE_CONTEXT.level;
}

export function resolveRoundContextLimits(options = {}) {
  const level = normalizeContextLevel(options.level);
  const preset = ROUNDTABLE_CONTEXT_PRESETS[level] || ROUNDTABLE_CONTEXT_PRESETS.rich;

  const perAgentChars = parsePositiveInt(options.perAgentChars, preset.perAgentChars, 600);
  const totalChars = parsePositiveInt(options.totalChars, preset.totalChars, 2000);

  return {
    level,
    perAgentChars,
    totalChars: Math.max(totalChars, perAgentChars),
  };
}

export function clampRoundCount(rounds) {
  const n = Number(rounds);
  if (!Number.isFinite(n)) return 2;
  if (n < 1) return 1;
  return Math.floor(n);
}

export function resolveRound1ForceNew(participantPolicy, round1Policy) {
  if (participantPolicy === undefined || participantPolicy === null) {
    return Boolean(round1Policy);
  }
  return Boolean(participantPolicy);
}

export function resolveRoundStageTimeoutSecs(timeoutSecs) {
  const parsed = Number(timeoutSecs);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return DEFAULT_ROUND_STAGE_TIMEOUT_SECS;
  }

  return Math.min(
    MAX_ROUND_STAGE_TIMEOUT_SECS,
    Math.max(MIN_ROUND_STAGE_TIMEOUT_SECS, Math.floor(parsed)),
  );
}

export function resolveRoundStageMinSuccesses(minSuccesses, participantCount) {
  const defaultMin = Math.max(1, Math.min(3, participantCount));
  const parsed = Number(minSuccesses);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return defaultMin;
  }

  return Math.max(1, Math.min(Math.floor(parsed), participantCount));
}

function alphaLabel(index) {
  let value = Number(index) + 1;
  if (!Number.isFinite(value) || value <= 0) return 'A';

  let out = '';
  while (value > 0) {
    const mod = (value - 1) % 26;
    out = String.fromCharCode(65 + mod) + out;
    value = Math.floor((value - 1) / 26);
  }

  return out;
}

function toBoolean(value, fallback) {
  if (value === undefined || value === null) return fallback;
  return Boolean(value);
}

function timestampRunID() {
  return new Date().toISOString().replace(/[:.]/g, '-');
}

function artifactRootPath(worktree, parentKey, runID) {
  const digest = createHash('sha1').update(String(parentKey)).digest('hex').slice(0, 12);
  return join(worktree, '.three', 'roundtable-artifacts', digest, runID);
}

function writeJSONFile(path, payload) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
}

function buildStageTimeoutResult(task, timeoutSecs) {
  const agent = normalizeAgentName(task.agent ?? task.role ?? task.name);
  return {
    name: task.name || agent,
    role: agent,
    agent,
    success: false,
    resumed: false,
    session_id: null,
    message_id: null,
    model: typeof task.model === 'string' ? task.model : null,
    text: '',
    error: `round stage timeout (${timeoutSecs}s)`,
    stage_timeout: true,
  };
}

function cleanText(parts) {
  if (!Array.isArray(parts)) return '';
  return parts
    .filter((part) => part && (part.type === 'text' || part.type === 'reasoning'))
    .map((part) => String(part.text ?? '').trim())
    .filter(Boolean)
    .join('\n\n');
}

function sanitizeCarryoverText(text) {
  return String(text ?? '')
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((line) => line.trimEnd())
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function truncateText(text, maxChars) {
  const normalized = sanitizeCarryoverText(text);
  if (normalized.length <= maxChars) {
    return {
      text: normalized,
      truncated: false,
    };
  }

  const hardCap = Math.max(maxChars - 16, 0);
  let cut = normalized.lastIndexOf('\n\n', hardCap);
  if (cut < Math.floor(hardCap * 0.55)) {
    cut = hardCap;
  }

  return {
    text: `${normalized.slice(0, Math.max(0, cut)).trimEnd()}\n...[truncated]`,
    truncated: true,
  };
}

function truncateTextKeepingTail(text, maxChars) {
  const normalized = sanitizeCarryoverText(text);
  if (normalized.length <= maxChars) {
    return {
      text: normalized,
      truncated: false,
    };
  }

  const marker = '...[earlier rounds truncated]\n';
  const budget = Math.max(maxChars - marker.length, 0);
  const tailStartFloor = Math.max(0, normalized.length - budget);
  let start = normalized.indexOf('=== Round ', tailStartFloor);
  if (start < 0 || start > normalized.length - Math.floor(budget * 0.6)) {
    start = tailStartFloor;
  }

  return {
    text: `${marker}${normalized.slice(start).trimStart()}`.slice(0, maxChars),
    truncated: true,
  };
}

async function pluginLog(client, level, message, metadata = undefined) {
  if (!client?.app?.log) return;
  try {
    await client.app.log({
      body: {
        service: 'three-opencode',
        level,
        message,
        ...(metadata ? { metadata } : {}),
      },
    });
  } catch {
    // Best effort only.
  }
}

export function computeParentStoreKey(sessionID, conversationID) {
  const sid = String(sessionID ?? '').trim();
  const cid = String(conversationID ?? '').trim();
  if (!sid) return '';
  return cid ? `${sid}::${cid}` : sid;
}

export function buildRoundPrompt({
  round,
  totalRounds,
  agent,
  topic,
  previousContext,
  anonymousViewpoints,
}) {
  const header = `ROUND ${round}/${totalRounds}`;
  if (round <= 1) {
    return `${header}
TOPIC:
${topic}

You are ${agent}.

Reply with:
1) Position (1-2 sentences)
2) Key reasons (bullets)
3) Risks (bullets)
4) Recommendation (1 sentence)`;
  }

  const peerRef = anonymousViewpoints ? 'response labels' : 'named peers';
  const contextTitle = anonymousViewpoints
    ? 'Previous round viewpoints (anonymized substantial excerpts):'
    : 'Previous round viewpoints (substantial excerpts):';

  return `${header}
TOPIC:
${topic}

${contextTitle}
${previousContext || '(none)'}

You are ${agent}.

Reply with:
1) Do you change your position?
2) Agreements with ${peerRef} (bullets)
3) Remaining disagreement with ${peerRef} (bullets)
4) Updated recommendation`;
}

export function summarizeRound(contributions) {

  const total = contributions.length;
  const success = contributions.filter((item) => item.success).length;
  const failed = total - success;

  const lines = [
    `Participants: ${total}`,
    `Success: ${success}`,
    `Failed: ${failed}`,
  ];

  for (const c of contributions) {
    if (c.success) {
      const snippet = String(c.text ?? '').split('\n')[0].slice(0, 120);
      lines.push(`- ${c.agent}: ${snippet || '(empty)'}`);
    } else {
      lines.push(`- ${c.agent || c.role || 'unknown'}: ERROR ${c.error || 'unknown'}`);
    }
  }

  return lines.join('\n');
}

export function buildRoundContext(contributions, options = {}) {
  const limits = resolveRoundContextLimits(options);
  const anonymousViewpoints = Boolean(options.anonymousViewpoints);
  const successItems = contributions.filter((item) => item.success);
  const failedItems = contributions.filter((item) => !item.success);

  const lines = [
    `Participants: ${contributions.length}`,
    `Success: ${successItems.length}`,
    `Failed: ${failedItems.length}`,
    `Context policy: level=${limits.level}, per_agent<=${limits.perAgentChars}, total<=${limits.totalChars}, anonymous=${anonymousViewpoints}`,
    '',
  ];

  const labelMap = [];

  if (successItems.length === 0) {
    lines.push('No successful viewpoints in previous round.');
  } else {
    const fairShare = Math.max(
      800,
      Math.floor(limits.totalChars / Math.max(1, successItems.length)),
    );
    const perAgentBudget = Math.min(limits.perAgentChars, fairShare);

    successItems.forEach((item, index) => {
      const label = anonymousViewpoints ? `Response ${alphaLabel(index)}` : item.agent;
      labelMap.push({ label, agent: item.agent });

      const excerpt = truncateText(item.text, perAgentBudget);
      lines.push(`[${label}]`);
      lines.push(excerpt.text || '(empty)');
      lines.push('');
    });
  }

  if (failedItems.length > 0) {
    lines.push('Errors from previous round:');
    for (const item of failedItems) {
      lines.push(`- ${item.agent || item.role || 'unknown'}: ${item.error || 'unknown error'}`);
    }
  }

  const merged = lines.join('\n').trim();
  const trimmed = truncateText(merged, limits.totalChars);

  return {
    text: trimmed.text,
    stats: {
      level: limits.level,
      per_agent_chars: limits.perAgentChars,
      total_chars_limit: limits.totalChars,
      context_chars: trimmed.text.length,
      truncated: trimmed.truncated,
      success_count: successItems.length,
      failed_count: failedItems.length,
      anonymous_viewpoints: anonymousViewpoints,
      label_map: anonymousViewpoints ? labelMap : null,
    },
  };
}

export function mergeRoundContext(previousContext, currentRoundContext, options = {}) {

  const limits = resolveRoundContextLimits(options);
  const chunks = [];
  const prev = sanitizeCarryoverText(previousContext);
  const next = sanitizeCarryoverText(currentRoundContext);

  if (prev) chunks.push(prev);
  if (next) chunks.push(next);
  if (chunks.length === 0) {
    return {
      text: '',
      truncated: false,
      total_chars_limit: limits.totalChars,
    };
  }

  const merged = truncateTextKeepingTail(chunks.join('\n\n'), limits.totalChars);
  return {
    text: merged.text,
    truncated: merged.truncated,
    total_chars_limit: limits.totalChars,
  };
}


export async function fetchAgents(client, directory) {
  const result = await client.app.agents({ directory });
  const err = unwrapError(result);
  if (err) throw new Error(String(err));

  const data = unwrapData(result);
  return Array.isArray(data) ? data : [];
}

export async function resolveAgents(client, directory) {
  const agents = await fetchAgents(client, directory);
  const catalog = buildAgentCatalog(agents);
  return { agents, catalog };
}

async function ensureChildSession({
  client,
  directory,
  parentSessionID,
  parentKey,
  agent,
  forceNew,
  explicitSessionID,
  store,
}) {
  if (!forceNew && explicitSessionID) {
    return {
      sessionID: explicitSessionID,
      resumed: true,
      created: false,
    };
  }

  if (!forceNew) {
    const mapped = getRoleSession(store, parentKey, agent);
    if (mapped) {
      try {
        const check = await client.session.get({ path: { id: mapped }, query: { directory } });
        if (!unwrapError(check)) {
          return {
            sessionID: mapped,
            resumed: true,
            created: false,
          };
        }
        removeRoleSession(store, parentKey, agent);
      } catch {
        removeRoleSession(store, parentKey, agent);
      }
    }
  }

  const created = await client.session.create({
    body: {
      parentID: parentSessionID,
      title: `three:${agent}`,
      permission: [{ permission: 'question', action: 'deny', pattern: '*' }],
    },
    query: { directory },
  });

  const createErr = unwrapError(created);
  if (createErr) throw new Error(String(createErr));

  const data = unwrapData(created);
  const sessionID = String(data?.id ?? '').trim();
  if (!sessionID) throw new Error(`failed to create child session for agent '${agent}'`);

  setRoleSession(store, parentKey, agent, sessionID);

  return {
    sessionID,
    resumed: false,
    created: true,
  };
}

async function runOneTask({
  client,
  directory,
  parentSessionID,
  parentKey,
  store,
  catalog,
  task,
}) {
  const requestedAgent = normalizeAgentName(task.agent ?? task.role);
  const prompt = String(task.PROMPT ?? task.prompt ?? '').trim();

  if (!requestedAgent) {
    return {
      name: task.name || '',
      role: '',
      agent: '',
      success: false,
      error: 'agent is required',
    };
  }

  const resolved = resolveRequestedAgent(requestedAgent, catalog);
  if (resolved.error || !resolved.agent) {
    return {
      name: task.name || requestedAgent,
      role: requestedAgent,
      agent: requestedAgent,
      success: false,
      error: resolved.error || `unknown agent '${requestedAgent}'`,
    };
  }

  const agentName = resolved.agent.name;

  if (!prompt) {
    return {
      name: task.name || agentName,
      role: agentName,
      agent: agentName,
      success: false,
      error: 'PROMPT is required',
    };
  }

  const model = parseModelRef(task.model);

  try {
    const sessionState = await ensureChildSession({
      client,
      directory,
      parentSessionID,
      parentKey,
      agent: agentName,
      forceNew: Boolean(task.force_new_session),
      explicitSessionID: task.SESSION_ID,
      store,
    });

    const promptResult = await client.session.prompt({
      path: { id: sessionState.sessionID },
      body: {
        agent: agentName,
        parts: [{ type: 'text', text: prompt }],
        ...(model ? { model: { providerID: model.providerID, modelID: model.modelID } } : {}),
        ...(model?.variant ? { variant: model.variant } : {}),
      },
      query: { directory },
    });

    const promptErr = unwrapError(promptResult);
    if (promptErr) {
      throw new Error(String(promptErr));
    }

    const data = unwrapData(promptResult) ?? {};
    const info = data.info ?? {};
    const text = cleanText(data.parts);

    setRoleSession(store, parentKey, agentName, sessionState.sessionID, {
      agent: agentName,
      lastMessageID: info.id,
      lastUpdatedAt: new Date().toISOString(),
    });

    return {
      name: task.name || agentName,
      role: agentName,
      agent: agentName,
      success: true,
      resumed: sessionState.resumed,
      session_id: sessionState.sessionID,
      message_id: info.id,
      model: formatModelRef(model),
      text,
      error: null,
    };
  } catch (error) {
    return {
      name: task.name || agentName,
      role: agentName,
      agent: agentName,
      success: false,
      resumed: false,
      session_id: null,
      message_id: null,
      model: formatModelRef(model),
      text: '',
      error: String(error?.message || error),
    };
  }
}

export async function runBatch({
  client,
  directory,
  worktree,
  parentSessionID,
  conversationID,
  tasks,
}) {
  const { agents, catalog } = await resolveAgents(client, directory);
  const parentKey = computeParentStoreKey(parentSessionID, conversationID);
  if (!parentKey) throw new Error('parent session id is required');

  const store = readSessionStore(worktree);
  const startedAt = Date.now();

  await pluginLog(client, 'info', `[batch] start tasks=${tasks.length}`, {
    parentSessionID,
    conversationID: conversationID || null,
  });

  let completed = 0;
  const pending = tasks.map(async (task) => {
    const result = await runOneTask({
      client,
      directory,
      parentSessionID,
      parentKey,
      store,
      catalog,
      task,
    });

    completed += 1;
    await pluginLog(client, result.success ? 'info' : 'warn',
      `[batch] completed ${result.agent || result.role} (${completed}/${tasks.length}) status=${result.success ? 'ok' : 'error'}`,
      { agent: result.agent || null, error: result.error || null });

    return result;
  });

  const results = await Promise.all(pending);
  writeSessionStore(worktree, store);

  const failed = results.filter((item) => !item.success).length;

  return {
    success: failed === 0,
    operation: 'batch',
    duration_ms: Date.now() - startedAt,
    directory,
    parent_session_id: parentSessionID,
    store_path: sessionStorePath(worktree),
    callable_agents: catalog.callable,
    primary_agents: catalog.primary,
    available_agents: agents.map((a) => a.name),
    results,
    error: failed > 0 ? 'one or more tasks returned an error' : null,
  };
}

function parseParticipants(input, catalog) {
  const defaultParticipants = catalog.callable.map((item) => ({
    agent: item.name,
    name: item.name,
  }));

  if (!Array.isArray(input) || input.length === 0) {
    return defaultParticipants;
  }

  const out = [];
  for (const item of input) {
    if (typeof item === 'string') {
      const agent = normalizeAgentName(item);
      out.push({ agent, name: agent });
      continue;
    }

    if (item && typeof item === 'object') {
      const agent = normalizeAgentName(item.agent ?? item.role ?? item.name);
      const hasForceNew = Object.prototype.hasOwnProperty.call(item, 'force_new_session');
      out.push({
        agent,
        name: String(item.name ?? agent),
        force_new_session: hasForceNew ? item.force_new_session : undefined,
        model: item.model,
      });
    }
  }

  return out.filter((p) => p.agent);
}

async function runRoundTasksWithStageTimeout({
  client,
  directory,
  parentSessionID,
  parentKey,
  store,
  catalog,
  tasks,
  round,
  totalRounds,
  timeoutSecs,
  minSuccesses,
}) {
  if (tasks.length === 0) {
    return {
      results: [],
      stage: {
        timeout_secs: timeoutSecs,
        min_successes: minSuccesses,
        success_count: 0,
        timed_out_count: 0,
        extended_wait: false,
      },
    };
  }

  const timeoutMs = resolveRoundStageTimeoutSecs(timeoutSecs) * 1000;
  const resolvedMinSuccesses = resolveRoundStageMinSuccesses(minSuccesses, tasks.length);
  const extensionMs = Math.min(
    ROUND_STAGE_TIMEOUT_EXTENSION_CAP_MS,
    Math.floor(timeoutMs * 0.5),
  );

  let deadlineAt = Date.now() + timeoutMs;
  let extensionUsed = false;

  const pending = new Map();
  const results = new Array(tasks.length).fill(null);
  let successCount = 0;
  let timedOutCount = 0;
  let completed = 0;

  tasks.forEach((task, index) => {
    const promise = runOneTask({
      client,
      directory,
      parentSessionID,
      parentKey,
      store,
      catalog,
      task,
    }).then((result) => ({ index, result }));

    pending.set(index, promise);
  });

  while (pending.size > 0) {
    const remainingMs = deadlineAt - Date.now();

    if (remainingMs <= 0) {
      if (successCount >= resolvedMinSuccesses || extensionUsed || extensionMs <= 0) {
        break;
      }

      extensionUsed = true;
      deadlineAt = Date.now() + extensionMs;

      await pluginLog(client, 'warn',
        `[roundtable] round ${round}/${totalRounds} timeout reached, extending wait by ${Math.ceil(extensionMs / 1000)}s to hit min_successes=${resolvedMinSuccesses}`,
        {
          round,
          total_rounds: totalRounds,
          success_count: successCount,
          min_successes: resolvedMinSuccesses,
        });
      continue;
    }

    const timeoutToken = Symbol('round-stage-timeout');
    const settled = await Promise.race([
      ...pending.values(),
      new Promise((resolve) => {
        setTimeout(() => resolve(timeoutToken), remainingMs);
      }),
    ]);

    if (settled === timeoutToken) {
      continue;
    }

    const { index, result } = settled;
    if (!pending.has(index)) {
      continue;
    }

    pending.delete(index);
    results[index] = result;
    completed += 1;
    if (result.success) successCount += 1;

    await pluginLog(client, result.success ? 'info' : 'warn',
      `[roundtable] round ${round}/${totalRounds} completed ${result.agent || result.role} (${completed}/${tasks.length}) status=${result.success ? 'ok' : 'error'}`,
      { agent: result.agent || null, error: result.error || null });
  }

  if (pending.size > 0) {
    const unresolvedEntries = [...pending.entries()];

    for (const [index] of unresolvedEntries) {
      pending.delete(index);
      results[index] = buildStageTimeoutResult(tasks[index], resolveRoundStageTimeoutSecs(timeoutSecs));
      timedOutCount += 1;

      await pluginLog(client, 'warn',
        `[roundtable] round ${round}/${totalRounds} timed out ${tasks[index].agent || tasks[index].role}`,
        { round, timeout_secs: resolveRoundStageTimeoutSecs(timeoutSecs) });
    }

    const unresolvedPromises = unresolvedEntries.map(([, promise]) => promise);
    if (unresolvedPromises.length > 0) {
      void Promise.allSettled(unresolvedPromises);
    }
  }

  for (let i = 0; i < results.length; i += 1) {
    if (results[i]) continue;
    results[i] = {
      name: tasks[i].name || tasks[i].agent || `task-${i + 1}`,
      role: tasks[i].role || tasks[i].agent || '',
      agent: tasks[i].agent || tasks[i].role || '',
      success: false,
      resumed: false,
      session_id: null,
      message_id: null,
      model: typeof tasks[i].model === 'string' ? tasks[i].model : null,
      text: '',
      error: 'task completed without a result',
    };
  }

  return {
    results,
    stage: {
      timeout_secs: resolveRoundStageTimeoutSecs(timeoutSecs),
      min_successes: resolvedMinSuccesses,
      success_count: successCount,
      timed_out_count: timedOutCount,
      extended_wait: extensionUsed,
    },
  };
}

export async function runRoundtable({
  client,
  directory,
  worktree,
  parentSessionID,
  conversationID,
  topic,
  participants,
  rounds,
  round1ForceNew,
  roundContextLevel,
  roundContextMaxChars,
  perAgentContextMaxChars,
  roundStageTimeoutSecs,
  roundStageMinSuccesses,
  round2OnlyStage1Success,
  roundAnonymousViewpoints,
  persistRoundArtifacts,
}) {
  const cleanTopic = String(topic ?? '').trim();
  if (!cleanTopic) throw new Error('TOPIC is required');

  const { agents, catalog } = await resolveAgents(client, directory);
  const parsedParticipants = parseParticipants(participants, catalog);
  if (parsedParticipants.length === 0) throw new Error('no valid participants');

  const totalRounds = clampRoundCount(rounds);
  const contextLimits = resolveRoundContextLimits({
    level: roundContextLevel,
    totalChars: roundContextMaxChars,
    perAgentChars: perAgentContextMaxChars,
  });

  const stageTimeoutSecs = resolveRoundStageTimeoutSecs(roundStageTimeoutSecs);
  const requireStage1SuccessForRound2 = toBoolean(round2OnlyStage1Success, true);
  const anonymousViewpoints = toBoolean(roundAnonymousViewpoints, false);
  const persistArtifacts = toBoolean(persistRoundArtifacts, true);

  const store = readSessionStore(worktree);
  const parentKey = computeParentStoreKey(parentSessionID, conversationID);
  if (!parentKey) throw new Error('parent session id is required');

  const roundtableRunID = timestampRunID();
  const roundtableArtifactDir = persistArtifacts
    ? artifactRootPath(worktree, parentKey, roundtableRunID)
    : null;

  if (roundtableArtifactDir) {
    writeJSONFile(join(roundtableArtifactDir, 'run.start.json'), {
      operation: 'roundtable',
      run_id: roundtableRunID,
      started_at: new Date().toISOString(),
      topic: cleanTopic,
      total_rounds: totalRounds,
      parent_session_id: parentSessionID,
      conversation_id: conversationID || null,
      context_policy: contextLimits,
      stage_policy: {
        timeout_secs: stageTimeoutSecs,
        min_successes: roundStageMinSuccesses ?? null,
        require_stage1_success_for_round2: requireStage1SuccessForRound2,
      },
      anonymous_viewpoints: anonymousViewpoints,
      participants: parsedParticipants.map((p) => ({
        name: p.name,
        agent: p.agent,
        force_new_session: p.force_new_session,
        model: p.model || null,
      })),
    });
  }

  const roundsOut = [];
  let previousContext = '';
  let abortReason = null;
  const stage1SuccessAgents = new Set();

  for (let round = 1; round <= totalRounds; round += 1) {
    let roundParticipants = parsedParticipants;
    if (round >= 2 && requireStage1SuccessForRound2) {
      roundParticipants = parsedParticipants.filter((p) => stage1SuccessAgents.has(p.agent));
    }

    if (roundParticipants.length === 0) {
      abortReason = 'round 2+ has no participants because no stage 1 participant succeeded';
      roundsOut.push({
        round,
        summary: 'Participants: 0\nSuccess: 0\nFailed: 0',
        context_stats: null,
        stage: {
          timeout_secs: stageTimeoutSecs,
          min_successes: 0,
          success_count: 0,
          timed_out_count: 0,
          extended_wait: false,
        },
        participant_count: 0,
        contributions: [],
        failed_count: 0,
        skipped: true,
        abort_reason: abortReason,
      });
      break;
    }

    await pluginLog(client, 'info', `[roundtable] round ${round}/${totalRounds} started`, {
      participant_count: roundParticipants.length,
      stage_timeout_secs: stageTimeoutSecs,
      anonymous_viewpoints: anonymousViewpoints,
    });

    const tasks = roundParticipants.map((p) => ({
      name: p.name,
      role: p.agent,
      agent: p.agent,
      PROMPT: buildRoundPrompt({
        round,
        totalRounds,
        agent: p.agent,
        topic: cleanTopic,
        previousContext,
        anonymousViewpoints,
      }),
      model: p.model,
      force_new_session: round === 1
        ? resolveRound1ForceNew(p.force_new_session, round1ForceNew)
        : false,
    }));

    const roundRun = await runRoundTasksWithStageTimeout({
      client,
      directory,
      parentSessionID,
      parentKey,
      store,
      catalog,
      tasks,
      round,
      totalRounds,
      timeoutSecs: stageTimeoutSecs,
      minSuccesses: roundStageMinSuccesses,
    });

    const results = roundRun.results;

    if (round === 1) {
      for (const item of results) {
        if (item.success && item.agent) stage1SuccessAgents.add(item.agent);
      }
    }

    const summary = summarizeRound(results);
    const roundContext = buildRoundContext(results, {
      ...contextLimits,
      anonymousViewpoints,
    });
    const carryover = mergeRoundContext(
      previousContext,
      `=== Round ${round} viewpoints ===\n${roundContext.text}`,
      contextLimits,
    );

    const roundOut = {
      round,
      summary,
      context_stats: {
        ...roundContext.stats,
        carryover_chars: carryover.text.length,
        carryover_truncated: carryover.truncated,
      },
      stage: roundRun.stage,
      participant_count: roundParticipants.length,
      contributions: results,
      failed_count: results.filter((r) => !r.success).length,
    };

    roundsOut.push(roundOut);

    if (roundtableArtifactDir) {
      writeJSONFile(
        join(roundtableArtifactDir, `round-${String(round).padStart(2, '0')}.json`),
        roundOut,
      );
    }

    previousContext = carryover.text;
  }

  writeSessionStore(worktree, store);

  const flat = roundsOut.flatMap((r) => r.contributions || []);
  const failed = flat.filter((item) => !item.success).length;
  const success = failed === 0 && !abortReason;

  const output = {
    success,
    operation: 'roundtable',
    topic: cleanTopic,
    rounds: roundsOut,
    directory,
    parent_session_id: parentSessionID,
    store_path: sessionStorePath(worktree),
    artifact_dir: roundtableArtifactDir,
    round_context_policy: {
      ...contextLimits,
      anonymous_viewpoints: anonymousViewpoints,
    },
    round_stage_policy: {
      timeout_secs: stageTimeoutSecs,
      min_successes: roundStageMinSuccesses ?? null,
      require_stage1_success_for_round2: requireStage1SuccessForRound2,
    },
    round2_plus_forced_resume: true,
    round2_3_forced_resume: true,
    callable_agents: catalog.callable,
    primary_agents: catalog.primary,
    available_agents: agents.map((a) => a.name),
    aborted_reason: abortReason,
    error: abortReason || (failed > 0 ? 'one or more participant calls failed' : null),
  };

  if (roundtableArtifactDir) {
    writeJSONFile(join(roundtableArtifactDir, 'run.complete.json'), {
      ...output,
      completed_at: new Date().toISOString(),
      total_failed_contributions: failed,
    });
  }

  return output;
}
