import { tool } from '@opencode-ai/plugin';

import {
  runRoundtable,
} from './src/orchestrator.mjs';

const THREE_TOOL_IDS = new Set(['three_native_roundtable']);
const TOOL_METADATA_TTL_MS = 15 * 60 * 1000;
const pendingToolMetadata = new Map();


const STRICT_COMMAND_NAMES = new Set(['roundtable']);
const STRICT_COMMAND_TTL_MS = 20 * 60 * 1000;
const strictCommandSessions = new Map();

function normalizeCommandName(command) {
  return String(command ?? '').trim().replace(/^\/+/, '').toLowerCase();
}

function cleanupStrictCommandSessions() {
  const now = Date.now();
  for (const [key, entry] of strictCommandSessions.entries()) {
    if (now - entry.storedAt > STRICT_COMMAND_TTL_MS) {
      strictCommandSessions.delete(key);
    }
  }
}

function markStrictCommandSession(sessionID, commandName) {
  const sid = asNonEmptyString(sessionID);
  if (!sid) return;

  cleanupStrictCommandSessions();
  strictCommandSessions.set(sid, {
    storedAt: Date.now(),
    command: commandName,
  });
}

function readStrictCommandSession(sessionID) {
  const sid = asNonEmptyString(sessionID);
  if (!sid) return null;

  cleanupStrictCommandSessions();
  return strictCommandSessions.get(sid) ?? null;
}

function asNonEmptyString(value) {
  const text = typeof value === 'string' ? value.trim() : '';
  return text.length > 0 ? text : null;
}

function metadataKey(sessionID, callID) {
  const sid = asNonEmptyString(sessionID);
  const cid = asNonEmptyString(callID);
  if (!sid || !cid) return null;
  return `${sid}:${cid}`;
}

function cleanupPendingMetadata() {
  const now = Date.now();
  for (const [key, entry] of pendingToolMetadata.entries()) {
    if (now - entry.storedAt > TOOL_METADATA_TTL_MS) {
      pendingToolMetadata.delete(key);
    }
  }
}

function stagePendingMetadata(sessionID, callID, payload) {
  const key = metadataKey(sessionID, callID);
  if (!key || !payload) return;

  cleanupPendingMetadata();
  pendingToolMetadata.set(key, {
    storedAt: Date.now(),
    payload,
  });
}

function takePendingMetadata(sessionID, callID) {
  const key = metadataKey(sessionID, callID);
  if (!key) return null;

  const hit = pendingToolMetadata.get(key);
  if (!hit) return null;

  pendingToolMetadata.delete(key);
  return hit.payload;
}

function collectSessionIDs(items) {
  const unique = new Set();
  const ordered = [];

  for (const item of items) {
    const sessionID = asNonEmptyString(item?.session_id);
    if (!sessionID || unique.has(sessionID)) continue;
    unique.add(sessionID);
    ordered.push(sessionID);
  }

  return ordered;
}

function collectAgentSessions(items) {
  const byAgent = {};
  for (const item of items) {
    if (!item?.success) continue;
    const agent = asNonEmptyString(item.agent) || asNonEmptyString(item.role);
    const sessionID = asNonEmptyString(item.session_id);
    if (!agent || !sessionID || byAgent[agent]) continue;
    byAgent[agent] = sessionID;
  }
  return byAgent;
}

function buildRoundtableMetadata(out) {
  const rounds = Array.isArray(out?.rounds) ? out.rounds : [];
  const contributions = rounds.flatMap((round) =>
    Array.isArray(round?.contributions) ? round.contributions : []);

  const successCount = contributions.filter((item) => item?.success).length;
  const failedCount = Math.max(0, contributions.length - successCount);
  const sessionIds = collectSessionIDs(contributions);
  const agentSessions = collectAgentSessions(contributions);
  const participantCount = rounds.reduce((max, round) => {
    const value = Number(round?.participant_count ?? 0);
    return Number.isFinite(value) ? Math.max(max, value) : max;
  }, 0);

  return {
    operation: 'three-roundtable',
    round_count: rounds.length,
    participant_count: participantCount,
    contribution_count: contributions.length,
    success_count: successCount,
    failed_count: failedCount,
    aborted_reason: out?.aborted_reason ?? null,
    round2_plus_forced_resume: out?.round2_plus_forced_resume ?? true,
    sessionId: sessionIds[0],
    sessionIds,
    agentSessions,
  };
}

function emitToolMetadata(toolContext, payload) {
  if (!payload) return;

  if (typeof toolContext?.metadata === 'function') {
    toolContext.metadata(payload);
  }

  stagePendingMetadata(toolContext?.sessionID, toolContext?.callID, payload);
}

function appendThreeCommands(config) {
  if (!config || typeof config !== 'object') return;
  if (!config.command || typeof config.command !== 'object') {
    config.command = {};
  }

  const commands = config.command;
  const legacyCommandNames = [
    'three-batch',
    'three_batch',
    'three-roundtable',
    'three_roundtable',
    'three:info',
    'three:conductor',
    'three:builder',
    'three:critic',
    'three:oracle',
    'three:researcher',
    'three:reviewer',
    'three:sprinter',
  ];

  for (const legacyName of legacyCommandNames) {
    if (Object.prototype.hasOwnProperty.call(commands, legacyName)) {
      delete commands[legacyName];
    }
  }

  const roundtableSpec = {
    description: 'Sisyphus roundtable: hard-route through task/background_output multi-round orchestration.',
    template: [
      'You are sisyphus (main orchestrator).',
      'MUST run roundtable via task(...) + background_output(...) so each participant thread is clickable/traceable in TUI.',
      'Participant turns MUST use task(subagent_type="<agent>", run_in_background=true, prompt=...); NEVER use task(agent=...) or category for participant turns.',
      'If you lack context, you MAY launch 1-2 prep tasks (including sisyphus-junior) before Round 1, but actual round participants must be explicit non-primary role agents.',
      'Do not let all participant turns be sisyphus-junior; keep at least 2 distinct named role agents unless user explicitly asks otherwise.',
      'Round 1: explicitly choose memory policy (new vs resume) and launch participant tasks in parallel.',
      'Round 2+: always continue the same participant session_id (no force-new reset).',
      'Carry substantial peer viewpoints into next-round prompts (not one-line summaries).',
      'If background_output times out, keep polling and continue existing participant sessions.',
      'Never call three_native_roundtable in this command.',
    ].join('\n'),
  };


  const specs = {
    'roundtable': roundtableSpec,
  };

  for (const [name, spec] of Object.entries(specs)) {
    const existing = commands[name] && typeof commands[name] === 'object'
      ? commands[name]
      : {};

    commands[name] = {
      ...existing,
      name,
      description: spec.description,
      template: spec.template,
    };
  }
}

export const ThreeOpenCodePlugin = async (ctx) => ({
  config: async (config) => {
    appendThreeCommands(config);
  },


  'command.execute.before': async (input) => {
    const commandName = normalizeCommandName(input?.command);
    if (!STRICT_COMMAND_NAMES.has(commandName)) return;
    markStrictCommandSession(input?.sessionID, commandName);
  },

  'tool.execute.after': async (input, output) => {
    if (!input || !output) return;
    if (!THREE_TOOL_IDS.has(String(input.tool))) return;

    const restored = takePendingMetadata(input.sessionID, input.callID);
    if (!restored) return;

    if (restored.title) {
      output.title = restored.title;
    }

    if (restored.metadata && typeof restored.metadata === 'object') {
      const current = output.metadata && typeof output.metadata === 'object'
        ? output.metadata
        : {};
      output.metadata = { ...current, ...restored.metadata };
    }
  },

  tool: {
    three_native_roundtable: tool({
      description: 'Run multi-round discussion across subagents with session reuse and rich cross-round context.',
      args: {
        TOPIC: tool.schema.string().optional(),
        topic: tool.schema.string().optional(),
        participants: tool.schema
          .array(
            tool.schema.union([
              tool.schema.string(),
              tool.schema.object({
                agent: tool.schema.string().optional(),
                role: tool.schema.string().optional(),
                name: tool.schema.string().optional(),
                force_new_session: tool.schema.boolean().optional(),
                model: tool.schema.string().optional(),
              }),
            ]),
          )
          .optional(),
        rounds: tool.schema.number().int().min(1).optional(),
        round1_force_new_session: tool.schema.boolean().optional(),
        round_stage_timeout_secs: tool.schema
          .number()
          .int()
          .min(15)
          .max(600)
          .optional()
          .describe('Round stage timeout in seconds (default 90).'),
        round_stage_min_successes: tool.schema
          .number()
          .int()
          .min(1)
          .optional()
          .describe('Minimum successful participants required before accepting timeout.'),
        round2_only_stage1_success: tool.schema
          .boolean()
          .optional()
          .describe('If true (default), only round-1 successful participants continue to round 2+.'),
        round_anonymous_viewpoints: tool.schema
          .boolean()
          .optional()
          .describe('If true, round 2+ carryover uses Response A/B labels instead of agent names.'),
        persist_round_artifacts: tool.schema
          .boolean()
          .optional()
          .describe('Persist per-round evidence files under .three/roundtable-artifacts (default true).'),
        round_context_level: tool.schema
          .string()
          .optional()
          .describe('compact | balanced | rich (default rich)'),
        round_context_max_chars: tool.schema
          .number()
          .int()
          .min(2000)
          .optional()
          .describe('Upper bound for all carryover text passed to next round'),
        per_agent_context_max_chars: tool.schema
          .number()
          .int()
          .min(600)
          .optional()
          .describe('Upper bound per participant carryover text passed to next round'),
        conversation_id: tool.schema.string().optional(),
        allow_native: tool.schema.boolean().optional(),
      },
      async execute(args, toolContext) {

        const strictCommand = readStrictCommandSession(toolContext?.sessionID);
        if (strictCommand && !Boolean(args.allow_native)) {
          return JSON.stringify({
            success: false,
            error: `three_native_roundtable is blocked during /${strictCommand.command}. Keep using task(...) + background_output(...) or call with allow_native=true.`,
          });
        }

        const topic = String(args.TOPIC ?? args.topic ?? '').trim();
        if (!topic) {
          return JSON.stringify({ success: false, error: 'TOPIC is required' });
        }

        emitToolMetadata(toolContext, {
          title: `three roundtable (${args.rounds ?? 2} rounds)`,
          metadata: {
            operation: 'three-roundtable',
            round1_force_new_session: Boolean(args.round1_force_new_session),
            round2_plus_force_new_session: false,
            round_context_level: args.round_context_level || 'rich',
            round_stage_timeout_secs: args.round_stage_timeout_secs ?? 90,
            round_stage_min_successes: args.round_stage_min_successes ?? null,
            round2_only_stage1_success: args.round2_only_stage1_success ?? true,
            round_anonymous_viewpoints: Boolean(args.round_anonymous_viewpoints),
            persist_round_artifacts: args.persist_round_artifacts ?? true,
          },
        });

        const out = await runRoundtable({
          client: ctx.client,
          directory: toolContext.directory || ctx.directory,
          worktree: toolContext.worktree || ctx.worktree,
          parentSessionID: toolContext.sessionID,
          conversationID: args.conversation_id,
          topic,
          participants: args.participants,
          rounds: args.rounds,
          round1ForceNew: args.round1_force_new_session,
          roundContextLevel: args.round_context_level,
          roundContextMaxChars: args.round_context_max_chars,
          perAgentContextMaxChars: args.per_agent_context_max_chars,
          roundStageTimeoutSecs: args.round_stage_timeout_secs,
          roundStageMinSuccesses: args.round_stage_min_successes,
          round2OnlyStage1Success: args.round2_only_stage1_success,
          roundAnonymousViewpoints: args.round_anonymous_viewpoints,
          persistRoundArtifacts: args.persist_round_artifacts,
        });

        const metadata = buildRoundtableMetadata(out);
        emitToolMetadata(toolContext, {
          title: `three roundtable (${metadata.round_count} rounds, ${metadata.success_count}/${metadata.contribution_count} ok)`,
          metadata,
        });

        return JSON.stringify(out, null, 2);
      },
    }),
  },
});

export default ThreeOpenCodePlugin;
