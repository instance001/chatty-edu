# Module Visual Load-In

Use `visual_load.json` when a module already has its **own standalone UI** and you want Chatty-EDU to host that exact UI inside the module tab.

This keeps the module self-contained while letting Chatty-EDU:

- discover it automatically
- launch it from a closable tab
- optionally build it first if the module advertises a build command
- keep a small bridge panel beside the hosted UI for EDU-side status and handoff visibility

Pair this with:

- `chatty-edu/docs/MODULE_BRIDGE.md`
- `chatty-edu/docs/MODULE_BRIDGE_SNIPPETS.md`
- `chatty-edu/docs/MODULE_BUILDER_CHECKLIST.md`
- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md`
- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`

That gives you the full loop:

- `manifest.json` = discovery
- `visual_load.json` = host the real UI
- `bridge/status.json` = let the standalone module report back when it wants to

## Folder layout

```text
chatty-edu/
  modules/
    your_module/
      manifest.json
      visual_load.json
      ...
```

## Current support

Supported today on Windows:

- `native_window`
- `webview`

Current behavior:

- `native_window` = Chatty-EDU launches the standalone app and docks that real desktop window into the tab
- `webview` = Chatty-EDU hosts the module's real browser-style dashboard in a bundled webview helper
- no `visual_load.json` = Chatty-EDU falls back to legacy `module.json` behavior

## Three display paths

### 1) Docked native app

Use this when the module already has a real desktop window.

### 2) Docked web dashboard

Use this when the module already has a real HTML/CSS/JS dashboard.

### 3) EDU fallback surface

Use this when the module is headless, older, markdown-based, or simply does not ship a standalone GUI.

## Why the split exists

The goal is portability without forcing every builder into the same UI stack:

- native desktop tools keep their native desktop feel
- browser-style tools keep their browser-style feel
- older or simpler EDU tools can still stay usable without building a full GUI host path

## `visual_load.json` schema

Native-window example:

```json
{
  "kind": "native_window",
  "auto_launch": true,
  "window_title_contains": "Study Lab",
  "notes": "Optional note shown above the hosted UI.",
  "build": {
    "program": "cargo",
    "args": ["build"],
    "cwd": "."
  },
  "launch": {
    "program": "target/debug/study_lab.exe",
    "cwd": "."
  }
}
```

Webview example:

```json
{
  "kind": "webview",
  "auto_launch": true,
  "title": "Study Lab",
  "file": "web/index.html",
  "notes": "This starter keeps module state inside the module and uses the optional bridge only for Chatty-EDU handoff."
}
```

Fields:

- `kind` - use `native_window` or `webview`
- `auto_launch` - if `true`, Chatty-EDU launches the module when the tab opens
- `title` - optional hosted window title (especially useful for `webview`)
- `url` - webview target URL
- `file` - module-local HTML file to open in a hosted webview
- `window_title_contains` - helps Chatty-EDU identify the correct window to dock
- `notes` - optional user-facing note in the host toolbar
- `build` - optional command shown as **Build UI**
- `launch` - command used to start the standalone module UI
- `serve` - optional background command started before a hosted webview opens
- `serve_wait_ms` - optional delay before the hosted webview opens

## Good defaults

For native Rust modules:

- `window_title_contains` should match the title string used by the app window
- `build.program` is usually `cargo`
- `launch.program` is usually `target/debug/<your_app>.exe`

For Python GUI modules:

- point `launch.program` at `py`, `python`, or `pythonw`
- put the script path in `args`
- make sure the script really opens a GUI window

For hosted webviews:

- use `file` for static HTML/JS/CSS
- use `url` plus optional `serve` for local server dashboards
- prefer a stable local URL such as `http://127.0.0.1:4173`

## Relationship to the bridge

`visual_load.json` only answers how to visually host the module.

The bridge still answers how the module reports back optional status and log context.

See:

- `chatty-edu/docs/MODULE_BRIDGE.md`
- `chatty-edu/docs/MODULE_VISUAL_LOAD_TEMPLATE.json`
- `chatty-edu/docs/MODULE_VISUAL_LOAD_WEBVIEW_TEMPLATE.json`
