# Module Builder Checklist

Use this as a quick preflight when turning any standalone tool into a Chatty-EDU-compatible module.

## 1) Pick a starter

- Read `chatty-edu/docs/MODULE_TEMPLATE_CHOOSER.md`
- Copy one starter into `chatty-edu/modules/`
  - `template_module/` for webview
  - `template_native_rust_module/` for native Rust
  - `template_python_module/` for native Python

## 2) Rename the basics

- rename the copied folder
- update `manifest.json`
  - `module_id`
  - `display_name`
  - `description`
- update the module title shown by the app/UI
- update any hardcoded `MODULE_ID` constant in code

## 3) Keep the module standalone first

Before worrying about Chatty-EDU:

- make sure the module runs by itself
- make sure its own UI/state works normally
- make sure it can save/load whatever it needs on its own

Rule of thumb:

- Chatty-EDU should host the module
- Chatty-EDU should not become the module's real runtime brain

## 4) Add visual hosting

Create or update `visual_load.json`.

Choose one:

- `webview` if the module is HTML/CSS/JS
- `native_window` if the module opens a real desktop window

Check:

- launch path is correct
- working directory is correct
- window title is stable enough for docking
- optional build command works if the module needs one

## 5) Add the human handshake

Create or update `HANDSHAKE.md`.

Make sure it explains:

- what the module is for
- what inputs it expects
- what outputs it produces
- what a good suspend handoff should contain

## 6) Add the optional bridge

Use:

- `chatty-edu/docs/MODULE_BRIDGE.md`
- `chatty-edu/docs/MODULE_BRIDGE_SNIPPETS.md`
- `chatty-edu/docs/MODULE_LOG_SOURCES_TEMPLATE.json`

Goal:

- module keeps owning its own UI/state
- module optionally reports a short `summary` + `snapshot`
- if the module already has useful logs, it can declare them for Chatty-EDU to tail
- Chatty-EDU reads that handoff when the module tab is left or closed

For webviews:

- use `window.chattyEduBridge.updateStatus(...)`

For native apps:

- write to `CHATTYEDU_BRIDGE_STATUS` if it exists

If the module already writes its own logs:

- add `bridge/log_sources.json`
- keep paths module-relative
- let Bookkeeper use the recent log tail for auto-generated handoff context

## 7) Keep the bridge lightweight

Good:

- one short paragraph in `summary`
- richer state in `snapshot`
- a few stable tags
- optional structured `payload`

Avoid:

- treating the bridge as the module's main database
- coupling the module tightly to Chatty-EDU internals
- assuming the bridge exists when the app runs standalone

## 8) Test both modes

Standalone check:

- launch the module by itself
- confirm it still works without Chatty-EDU

Hosted check:

- open Chatty-EDU
- use **Modules -> Rescan modules**
- open the module tab
- confirm the real UI appears in the tab
- use the module
- switch away from the tab
- confirm Chatty-EDU can read what happened

## 9) Confirm portability

Ask:

- if I remove the bridge logic, does the module still run standalone?
- if I remove `visual_load.json`, does the module still remain a valid standalone tool?

If yes, you are keeping the boundary clean.

## 10) Nice finishing touches

Recommended:

- add a zero-knowledge `USER_MANUAL.md` inside the module folder
- read `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md` before shipping
- do one pass with `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`
- keep filenames and labels obvious
- keep launch/build paths relative to the module folder when possible
- add a stable window title for native docking
- keep bridge updates meaningful, not noisy

## Ship-ready definition

A module is in good shape when:

- it runs standalone
- it can be hosted inside a Chatty-EDU tab
- it reports a clean suspend handoff through the optional bridge
- it stays simple enough that removing the plug cleanly removes Chatty-EDU compatibility without breaking the tool itself
