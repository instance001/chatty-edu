# Which Module Starter Should I Pick?

If you are building a new Chatty-EDU-compatible module, start here.

After you pick a starter, use:

- `chatty-edu/docs/MODULE_BUILDER_CHECKLIST.md`
- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md` when you are ready to share it
- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md` for a final quality pass
- `chatty-edu/docs/MODULE_RELEASE_NOTES_TEMPLATE.md` if you are publishing an update
- `chatty-edu/docs/CHANGELOG_TEMPLATE.md` if you want a running history file

All three starters keep the same core rule:

- your module owns its own UI and state
- Chatty-EDU only hosts it and reads the optional bridge handoff

That means:

- keep the plug -> Chatty-EDU-compatible
- remove the plug -> still works standalone

It does not mean:

- "if it works in Chatty-EDU, it should also be treated as a ChattyCog module"

Those hosts are intentionally separate by design, and the EDU side keeps stricter school and child-safety expectations.

It also means the starter should help you join a larger in-app loop:

- the main Chatty-EDU AI frames or reviews the work
- your module does specialist work in its own UI
- the result can hand cleanly to another EDU module, classroom step, or orchestrator pass

## Fast recommendation

Pick the first one that matches how you want to build:

### 1) `template_module/` - webview starter

Path:
- `chatty-edu/module_templates/template_module/`

Pick this if:
- you like HTML/CSS/JavaScript
- you want the fastest path to a nice-looking UI
- your module feels like a small dashboard, planner, lab surface, or editor
- you want easy portability into other web-style environments later

Good for:
- planners
- research dashboards
- note boards
- workflow consoles
- lightweight tools with forms, previews, and tabs

Tradeoffs:
- easiest to style
- easiest to iterate
- best for browser-style UI
- not ideal if you specifically want a desktop-native toolkit feel

### 2) `template_native_rust_module/` - native Rust desktop starter

Path:
- `chatty-edu/module_templates/template_native_rust_module/`

Pick this if:
- you want a fully native Rust app window
- you are comfortable with Rust
- you want stronger control over desktop behavior
- your module may grow into a heavier desktop tool

Good for:
- engineering tools
- advanced builders
- local utilities
- desktop-first lab apps
- long-term modules that may become substantial software

Tradeoffs:
- strongest native path
- very portable inside the Rust ecosystem
- more setup/compile time than a webview
- more code weight than the web starter

### 3) `template_python_module/` - native Python desktop starter

Path:
- `chatty-edu/module_templates/template_python_module/`

Pick this if:
- you prefer Python over Rust
- you want a standalone desktop tool quickly
- your logic/tooling already lives in Python
- you want something simple and practical first

Good for:
- research helpers
- analysis tools
- quick internal utilities
- lab scripts that need a window
- builder teams that are more comfortable in Python

Tradeoffs:
- very accessible
- easy to glue into existing Python tooling
- less polished by default than a custom webview UI
- depends on the local Python launcher/runtime

## Decision guide

If you are unsure, use this:

- want fastest UI progress -> `template_module/`
- want strongest native Rust path -> `template_native_rust_module/`
- want easiest Python path -> `template_python_module/`

## Shared compatibility pieces

No matter which starter you choose, you still get:

- `manifest.json` for discovery
- `visual_load.json` for hosted visual loading
- `HANDSHAKE.md` for the human-readable module contract
- optional `bridge/status.json` for handoff back to Chatty-EDU

Those pieces are there so a module can become part of a compounding workflow, not only a one-off utility tab.

## Suggested default

For most new builders, I recommend:

1. start with `template_module/`
2. only switch to native Rust or Python if you know you need a real desktop app

That usually gives the smoothest first success.
