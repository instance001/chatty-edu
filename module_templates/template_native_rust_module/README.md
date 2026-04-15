# Template Native Rust Module

This is a copy-and-start native-window module starter.

It lives in `chatty-edu/module_templates/`, so it will **not** auto-appear in Chatty-EDU until you copy it into `chatty-edu/modules/`.

## How to use it

1. Copy this folder into `chatty-edu/modules/`
2. Rename the copied folder
3. Update:
   - `manifest.json`
   - `visual_load.json`
   - `HANDSHAKE.md`
   - `Cargo.toml`
   - `src/main.rs`
4. Build it once:

```bash
cargo build
```

5. In Chatty-EDU, use **File -> Reload modules**

## What this starter demonstrates

- real native Rust desktop window via `eframe`
- standalone local state saved in the module folder
- optional bridge handoff through `CHATTYEDU_BRIDGE_STATUS`
- clean behavior inside Chatty-EDU or outside it

## First things to rename

- package/binary name in `Cargo.toml`
- `module_id` in `manifest.json`
- hosted window title in `visual_load.json`
- `MODULE_ID` constant in `src/main.rs`
