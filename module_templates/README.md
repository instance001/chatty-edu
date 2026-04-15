# Chatty-EDU Module Templates

This folder holds starter modules that are safe to copy from.

It sits outside `chatty-edu/modules/`, so Chatty-EDU will not auto-discover these templates as live modules.

Need help choosing?

- `chatty-edu/docs/MODULE_TEMPLATE_CHOOSER.md`
- `chatty-edu/docs/MODULE_BUILDER_CHECKLIST.md`
- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md`
- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`
- `chatty-edu/docs/MODULE_RELEASE_NOTES_TEMPLATE.md`
- `chatty-edu/docs/CHANGELOG_TEMPLATE.md`
- `chatty-edu/docs/MODULE_SUBMISSION_TEMPLATE.md`

## Included

- `template_module/` - a standalone webview starter with the Chatty-EDU bridge already wired in
- `template_native_rust_module/` - a native Rust `eframe` starter with the Chatty-EDU bridge already wired in
- `template_python_module/` - a native Python `tkinter` starter with the Chatty-EDU bridge already wired in

## Typical workflow

1. Copy the starter you want
2. Paste it into `chatty-edu/modules/`
3. Rename the copied folder
4. Update the copied module's `manifest.json`, `visual_load.json`, `network_capabilities.json`, `HANDSHAKE.md`, and its main UI file (`web/app.js`, `src/main.rs`, or `src/main.py`)
5. In Chatty-EDU, use **File -> Reload modules**

If you want working references before starting from a template, inspect the bundled EDU demos under `../modules/demo_*` and `../docs/DEMO_MODULES.md`.
