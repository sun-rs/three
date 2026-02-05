# CLI Output Modes and Parsing (Authoritative)

This document is the **single source of truth** for output modes and parsing rules.
All `docs/cli-*.md` files must link here and should not duplicate output details.

**Principle:** prefer single-shot output when available; use streaming only when required.

---

## Summary

| CLI | Mode used by three | CLI output modes | Session ID source | Message extraction | Notes |
| --- | --- | --- | --- | --- | --- |
| **claude** | `--output-format json` (single JSON) | `text` / `json` / `stream-json` | `session_id` | `result` | `stream-json` requires `--print --output-format stream-json --include-partial-messages --verbose` |
| **codex** | `--json` (JSONL stream) | `--json` (stream) / default text | `thread_id` | last `item.text` from `agent_message` | default text mixes thinking and output; avoid |
| **gemini** | `--output-format json` (single JSON) | `text` / `json` / `stream-json` | `session_id` | `response` | `stream-json` is multi-line events |
| **kimi** | `--output-format text --final-message-only` (single text) | `text` / `stream-json` | none | stdout text | output does not include session id |
| **opencode** | `--format json` (NDJSON stream) | `default` (text) / `json` (stream) | `part.sessionID` | last `part.text` from `type: text` | ignore tool/step events |

---

## Stream completion rule

For streaming outputs (Codex/OpenCode, or any CLI in stream mode), three waits for the CLI process to exit,
then selects the **last** candidate message that matches the extraction rule above. This avoids prematurely
returning partial messages.

---

## Details by CLI

### Claude

- Default: `--output-format json` (single JSON object).
- `json` content is equivalent to `stream-json`, but `stream-json` emits many events.
- If stream mode is ever enabled, parse `assistant`/`result` events or concatenate `content_block_delta`.

### Codex

- `--json` emits JSONL events; the final answer is the last `item.text` from `agent_message` events.
- Default text mode mixes reasoning with output and is hard to parse.

### Gemini

- `--output-format json` returns a single JSON object; parse `response`.
- `stream-json` outputs multiple lines and requires event aggregation.

### Kimi

- `--output-format text --final-message-only` returns a single text output.
- Neither text nor stream output reliably exposes a session id.

### OpenCode

- `--format json` emits NDJSON events; only `type: text` includes `part.text`.
- The final answer is the last `part.text` before process exit.
