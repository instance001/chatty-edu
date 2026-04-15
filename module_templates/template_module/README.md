# Template Module

This folder is a copy-and-start module skeleton.

It is stored in `chatty-edu/module_templates/` so it does **not** auto-load into Chatty-EDU.

## How to use it

1. Copy this folder into `chatty-edu/modules/`
2. Rename the copied folder
3. Update:
   - `manifest.json`
   - `visual_load.json`
   - `HANDSHAKE.md`
   - `web/app.js`
4. Open Chatty-EDU and use **File -> Reload modules**
5. Open your module from the **Modules** menu

## What this template demonstrates

- standalone browser-style module UI
- hosted in-tab `webview` inside Chatty-EDU
- module-owned local state via `localStorage`
- optional `bridge/status.json` handoff to Chatty-EDU

## What to rename first

- `module_id` in `manifest.json`
- display title in `manifest.json` and `visual_load.json`
- `MODULE_ID` in `web/app.js`
- summary wording in `web/app.js`

## Files

- `manifest.json` - discovery
- `visual_load.json` - host the real UI in a Chatty-EDU tab
- `HANDSHAKE.md` - human-facing department contract
- `STATE_TEMPLATE.md` - fallback notes if you later remove hosted UI
- `bridge/.gitkeep` - keeps the bridge folder in place
- `web/index.html` - standalone UI
- `web/styles.css` - styling
- `web/chattyedu_bridge.js` - optional hosted bridge helper
- `web/app.js` - your module UI/state logic
