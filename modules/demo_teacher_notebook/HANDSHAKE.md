# Teacher Notebook (Demo) Handshake

- Module type: hosted native window (`py -3 src/main.py`)
- Primary state: `state.json`
- Optional bridge: `bridge/status.json`
- Optional shared lesson state: `bridge/shared_state.json`
- Optional incoming mirrored lesson state: `bridge/incoming_shared_state.json`
- Optional module logs: `bridge/log_sources.json` -> `logs/session_log.md`

This module demonstrates the portable pattern we want from builders: the app owns its real UI and files, Chatty-EDU only reads the bridge when the plug is present, and incoming mirrored state can be applied without turning the module into a Chatty-EDU-specific app.
