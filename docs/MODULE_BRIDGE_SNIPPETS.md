# Module Bridge Snippet Pack

These are tiny copy-paste helpers for builders who want Chatty-EDU compatibility without giving up standalone portability.

If you want a full working starter instead of loose snippets, copy:

- `chatty-edu/module_templates/template_module/`
- `chatty-edu/module_templates/template_native_rust_module/`
- `chatty-edu/module_templates/template_python_module/`

If you are unsure which starter fits:

- `chatty-edu/docs/MODULE_TEMPLATE_CHOOSER.md`
- `chatty-edu/docs/MODULE_BUILDER_CHECKLIST.md`
- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md`
- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`

Use:

- `chatty-edu/docs/MODULE_BRIDGE_HELPER_RUST.rs` for native Rust desktop apps
- `chatty-edu/docs/MODULE_BRIDGE_HELPER_WEBVIEW.js` for hosted webviews

## Rust native-window helper

File:
- `chatty-edu/docs/MODULE_BRIDGE_HELPER_RUST.rs`

What it does:
- writes `bridge/status.json` only when Chatty-EDU is hosting the module
- does nothing in normal standalone runs

What your module needs:
- `serde_json`

Typical usage:

```rust
let _ = write_chattyedu_bridge_status(
    "your_module",
    "Short one-paragraph handoff.",
    "# Snapshot\n\nLonger state dump here.",
    &["builder", "native_window"],
    serde_json::json!({
        "active_tab": "Dashboard",
        "status": "Idle"
    }),
);
```

Good places to call it:
- after save
- after a meaningful state change
- once per UI update loop if you cache/deduplicate your own payload

## Webview helper

File:
- `chatty-edu/docs/MODULE_BRIDGE_HELPER_WEBVIEW.js`

What it does:
- calls the injected `window.chattyEduBridge` only when hosted by Chatty-EDU
- safely does nothing in a normal browser / standalone launch

Typical usage:

```js
updateChattyEduBridgeStatus(() => ({
  module_id: "your_module",
  summary: "Short one-paragraph handoff.",
  snapshot: "# Snapshot\n\nLonger state dump here.",
  tags: ["planning", "webview"],
  payload: {
    activeTab: "Overview",
    completion: 0.75
  }
}));
```

Good places to call it:
- after `saveState()`
- after recalculating previews
- after tab/phase changes inside the module UI

## Keep it simple

Recommended builder rule:

- module owns UI + state
- bridge only reports summary/snapshot
- Chatty-EDU reads the bridge but does not become the module's runtime brain

That gives you the clean behavior we want:

- add the plug -> Chatty-EDU-compatible
- remove the plug -> standalone only
