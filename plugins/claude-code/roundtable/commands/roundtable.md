---
description: Run a multi-role roundtable and synthesize a decision
---

# /roundtable:roundtable

Conductor roundtable mode entry (separate entry, shared responsibilities).

## Contract (must follow)

1. Follow `/roundtable:conductor` policy; this command only narrows mode to roundtable.
2. Reuse cached `mcp__roundtable__info` for `cd="."` + `client="claude"`; call once if missing.
3. One round = one `mcp__roundtable__roundtable` call (no manual serial role loops).
4. Round 1 memory mode is inferred by Conductor; if unclear, ask user before call.
5. Round 2+ always use `force_new_session=false`.
6. Keep participants stable across rounds unless user explicitly changes them.
7. Round 2+ TOPIC must include Round 1 summary + key disagreements.
8. Recommended: 2-3 rounds. Stop early on strong convergence. Continue if needed for complex topics.

## Steps

1. Read TOPIC.

2. Load/reuse `mcp__roundtable__info`; select enabled participants (>=3 roles).

3. Decide Round 1 memory mode (`true`/`false`) with reason.

4. **Round 1**: call `mcp__roundtable__roundtable` with chosen `force_new_session`.

5. **Analyze Round 1**:
   - Extract each role's position (1 sentence each)
   - Identify agreements (where 2+ roles align)
   - Identify disagreements (where roles conflict + why)
   - Assess: converged or divided?

6. **Round 2** (if needed):

   Build Round 2 TOPIC:
   ```
   TOPIC: [original]

   ROUND 1 SUMMARY:
   Agreements: [list]
   Disagreements: [list with role positions]

   TASK: Respond to above. Changed position? Cite specific arguments.
   ```

   Call `mcp__roundtable__roundtable` with `force_new_session=false`.

7. **Analyze Round 2**: Converged? Resolved? Need Round 3?

8. **Round 3+** (if needed): Repeat. Stop when converged or no new insights.

9. **Synthesis**:
   - Executive summary (2-3 sentences)
   - Key agreements + reasoning
   - Key disagreements + analysis
   - Evolution across rounds
   - Final recommendation + confidence
   - Trade-offs, dissent, open questions
