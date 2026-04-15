# Module Handshake Template (Drop-In)

Copy this file into your module folder (recommended name: `HANDSHAKE.md`) and fill it out.

Purpose:
- Give Chatty-EDU (and future module runners) a consistent "how to talk to this department" document.
- Provide the human-friendly suspend rundown format you want used for cross-module context.
- Pair with the optional portable bridge if your module keeps its own standalone UI/state.

## Module identity (required)

- **module_id**: `<same as manifest.json module_id>`
- **display_name**: `<same as manifest.json display_name>`

## What this module is for (required)

Describe the department's purpose in 1-3 sentences.

Example:
- "This department handles math + proofs + derivations. It prefers rigorous, step-by-step reasoning and explicit assumptions."

## Inputs this module expects (required)

List what users should provide to get work done.

Examples:
- Problem statement
- Constraints (time, cost, accuracy)
- Files placed into `Chatty_Sandbox/` (names + what they mean)
- Expected output format (Markdown, JSON, code patch, etc.)

## Outputs this module produces (required)

What does "done" look like?

Examples:
- A short report + citations list
- A code patch + commands to run
- A set of numbered steps + a checklist

## Operating rules / preferences (optional)

Add preferences that help the host app coordinate:

- Tone: `<concise | detailed | teaching>`
- Risk level: `<low | medium | high>`
- Default tags to use in logs: `<comma-separated>`
- Preferred file naming: `<e.g. notes/YYYY-MM-DD-topic.md>`

## Suspend rundown template (required)

When the user leaves this module tab, Chatty-EDU logs a "suspend rundown" event.
Write the *format* you want that summary to follow.

Guidelines:
- 1 paragraph max (aim for 3-6 sentences).
- Include what changed, what's pending, and the next action.
- Mention any important file paths (inside `Chatty_Sandbox/`) for continuity.

Template (fill in):

> **Status:** `<1 sentence current state>`
> **What changed:** `<1-2 sentences>`
> **Open questions:** `<1 sentence>`
> **Next action:** `<1 sentence>`
> **Artifacts:** `<optional: filenames/paths in Chatty_Sandbox/>`

## Cold log envelope hints (optional)

If you (or future tooling) append explicit events to the cold log, use:

- `module_id`: `<your module_id>`
- `event_type`: `<short stable string, e.g. "experiment", "draft", "decision">`
- `summary`: `<one paragraph>`
- `tags`: `<comma-separated list>`
- `payload_json`: `<optional JSON string with structured details>`

## Optional portable bridge (recommended for standalone UIs)

If your module keeps its own native UI or hosted webview state, you can also report back through:

- `bridge/status.json`

See:
- `chatty-edu/docs/MODULE_BRIDGE.md`
- `chatty-edu/docs/MODULE_BRIDGE_TEMPLATE.json`
- `chatty-edu/docs/MODULE_BRIDGE_SNIPPETS.md`

Rule of thumb:
- `HANDSHAKE.md` explains the department to humans
- `bridge/status.json` reports the current handoff state to Chatty-EDU
