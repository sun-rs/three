---
name: three-roundtable
description: Run a multi-role roundtable (1-3 rounds), feed back disagreements, and synthesize a decision
---

# three-roundtable

Use this for ambiguous, high-impact, or multi-tradeoff questions.

## Conductor mode

You are the conductor for this session. You must feed disagreement summaries back to each participant in later rounds before final synthesis.

## Default role pool (only if enabled in config)

| Role | Summary |
| --- | --- |
| `oracle` | Architecture, tech choices, long-term tradeoffs. |
| `builder` | Implementation, debugging, practical feasibility. |
| `researcher` | Evidence in code/docs/web with citations. |
| `reviewer` | Adversarial review for correctness and risk. |
| `critic` | Contrarian risk analysis and failure modes. |
| `sprinter` | Fast ideation and quick options (not exhaustive). |

## Execution rule (important)

- For each round, make **one** `mcp__three__batch` call that includes all participants.
- Do **not** emulate a round by looping serial `mcp__three__three` calls.
- Do **not** use `mcp__three__roundtable` for this skill; this skill requires conductor-managed multi-round feedback.
- Round 2 and Round 3 are **resume-only**: always `force_new_session=false` and always pass each participant's Round 1 `backend_session_id` as `session_id`.

## Required baseline

- Validate enabled roles with `mcp__three__info` first.
- Minimum participant count: 3 enabled roles.
- If `critic` is available, include it to reduce false consensus.

## Steps (up to 3 rounds)

1. Read topic from user request.

2. Call `mcp__three__info`:
   - `cd`: `.`
   - `client`: `"codex"`

3. Select 3-5 participants only from enabled roles (`info.roles` where `enabled=true`).

4. Round 1 (independent positions):
   - Call `mcp__three__batch` once with one task per participant:
     - `PROMPT`: Round 1 template
     - `role`: participant role
     - `name`: participant role (or unique label)
     - `force_new_session`: `true`
     - `client`: `"codex"`
     - `conversation_id`: pass when host can provide a stable main-chat id
   - Capture each participant `backend_session_id` for later rounds.

5. Analyze Round 1:
   - summarize each position
   - identify major disagreements and open questions
   - if strong consensus and no critical objection, you may stop

6. Round 2 (feedback disagreements, resume):
   - Call `mcp__three__batch` once with one task per participant:
     - `PROMPT`: Round 2 template with peer viewpoints
     - `role`: participant role
     - `name`: same participant label as Round 1
     - `session_id`: this participant Round 1 `backend_session_id`
     - `force_new_session`: `false`
     - `client`: `"codex"`
     - `conversation_id`: same value as Round 1 when available

7. Analyze Round 2:
   - stop if converged
   - otherwise continue to Round 3

8. Round 3 (final position, resume):
   - same as Round 2, but with final-confirmation prompt

9. Final report:
   - conclusion
   - key tradeoffs
   - recommended actions
   - dissenting views
   - open questions

## Prompt templates

Round 1:

```text
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

Round 2:

```text
ROUND 2 / 3 - Respond to disagreements
TOPIC:
{topic}

Summary of Round 1:
{participant_summaries}

Key disagreements / open questions:
{disagreement_list}

Please respond:
1) Do you keep your position? Why/why not?
2) Which opposing points are valid?
3) Any compromise or revised recommendation?
4) What evidence would resolve remaining uncertainty?
```

Round 3:

```text
ROUND 3 / 3 - Final position
Emerging consensus:
{consensus_summary}

Remaining concerns:
{remaining_concerns}

Please respond:
1) Final position (agree / disagree / conditional)
2) Non-negotiable constraints (bullets)
3) Last critical risk to highlight
```
