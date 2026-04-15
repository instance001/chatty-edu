# Template Python Module

This is a copy-and-start Python desktop module starter.

It lives in `chatty-edu/module_templates/`, so it will **not** auto-appear in Chatty-EDU until you copy it into `chatty-edu/modules/`.

## How to use it

1. Copy this folder into `chatty-edu/modules/`
2. Rename the copied folder
3. Update:
   - `manifest.json`
   - `visual_load.json`
   - `HANDSHAKE.md`
   - `src/main.py`
4. Make sure Python is installed and the `py` launcher works
5. In Chatty-EDU, use **File -> Reload modules**

## What this starter demonstrates

- real native Python desktop window via `tkinter`
- standalone local state saved in the module folder
- optional bridge handoff through `CHATTYEDU_BRIDGE_STATUS`
- clean behavior inside Chatty-EDU or outside it

## First things to rename

- `module_id` in `manifest.json`
- hosted window title in `visual_load.json`
- `MODULE_ID` in `src/main.py`
- the labels/summary wording in `src/main.py`
