---
description: Run a multi-role roundtable and synthesize a decision
---

# /three:roundtable

Use this when the question is ambiguous, multi-tradeoff, or benefits from multiple "souls".

## Conductor (you)

You are the Conductor. You host the session, pick participants, and synthesize the outcome.

## Preset roles (recommended)

| Role | Summary |
| --- | --- |
| `oracle` | Architecture, tech choices, long-term tradeoffs. |
| `builder` | Implementation, debugging, practical feasibility. |
| `researcher` | Evidence in code/docs/web with citations. |
| `reviewer` | Adversarial review for correctness and risk. |
| `critic` | Contrarian risk analysis and failure modes. |
| `sprinter` | Fast ideation and quick options (not exhaustive). |

## Steps (multi-round, up to 3)

1. Take the text after the command as `TOPIC`.

2. Call the MCP tool `mcp__three__info` with:
   - `cd`: `.`

   If any of these roles are missing or `enabled=false`, stop and explain:
   - `oracle`, `builder`, `researcher`
   - list available roles
   - suggest either adding the missing roles or choosing different roles and re-running

   If `critic` is missing, warn that the discussion may converge too early.

3. Pick 3–5 participants from the available roles.
   - Default set: `oracle`, `builder`, `researcher`
   - Add `critic` whenever possible (prevents false consensus)
   - Add `reviewer` for quality/risk review
   - Add `sprinter` for fast idea generation (non-exhaustive)

4. Round 1 (independent positions, new sessions):
   - For each participant, call `mcp__three__three` with:
     - `PROMPT`: use the Round 1 prompt template below
     - `cd`: `.`
     - `role`: participant role
     - `force_new_session`: `true`
   - Capture each output + `backend_session_id` (for round 2/3).

5. Analyze Round 1:
   - Summarize each participant’s position
   - Identify **major disagreements** and **open questions**
   - If consensus is strong **and** critic has no major objections → you may stop early and report.
   - Otherwise → proceed to Round 2.

6. Round 2 (disagreement feedback, resume sessions):
   - For each participant, call `mcp__three__three` with:
     - `PROMPT`: Round 2 prompt (includes other participants’ views + disagreements)
     - `cd`: `.`
     - `role`: participant role
     - `session_id`: that participant’s Round 1 session id
     - `force_new_session`: `false`

7. Analyze Round 2:
   - If disagreements are resolved or converging → you may stop early and report.
   - If material disagreements remain or more evidence is needed → proceed to Round 3.

8. Round 3 (final confirmation, resume sessions):
   - For each participant, call `mcp__three__three` with:
     - `PROMPT`: Round 3 prompt (emerging consensus + remaining concerns)
     - `cd`: `.`
     - `role`: participant role
     - `session_id`: that participant’s Round 1 session id
     - `force_new_session`: `false`

9. Final report (Conductor output):
   - **Conclusion** (1 paragraph)
   - **Key tradeoffs** (bullets)
   - **Recommendation / next actions** (bullets)
   - **Dissenting views** (bullets, if any)
   - **Open questions** (bullets, if any)

## Round 1 prompt (template)

```
ROUND 1 / 3
TOPIC:
{topic}

You are {role}.

Reply with:
1) Position (1-2 sentences)
2) Key arguments (3-5 bullets)
3) Risks / edge cases (2-3 bullets)
4) Recommendation (1 sentence)
5) Assumptions (bullets)
```

## Round 2 prompt (template)

```
ROUND 2 / 3 — Respond to disagreements
TOPIC:
{topic}

Summary of Round 1:
- Oracle: {oracle_position}
- Builder: {builder_position}
- Researcher: {researcher_position}
- Reviewer: {reviewer_position}
- Critic: {critic_position}
- Sprinter: {sprinter_position}

Key disagreements / open questions:
1) {disagreement_1}
2) {disagreement_2}
...

Please respond:
1) Do you keep your position? Why/why not?
2) Which opposing points are valid?
3) Any compromise or revised recommendation?
4) What evidence would resolve remaining uncertainty?
```

## Round 3 prompt (template)

```
ROUND 3 / 3 — Final position
Emerging consensus:
{consensus_summary}

Remaining concerns:
{remaining_concerns}

Please respond:
1) Final position (agree / disagree / conditional)
2) Non-negotiable constraints (bullets)
3) Any last critical risk we must highlight
```
