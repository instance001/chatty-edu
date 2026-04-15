# Revision Sprint (Demo) Handshake

- Module type: hosted webview demo
- Primary state: browser local storage
- Optional bridge: `bridge/status.json`
- Optional shared revision state: `bridge/shared_state.json`
- Optional incoming mirrored revision state: `bridge/incoming_shared_state.json`
- Roles: teacher, student

This module is intentionally simple: it owns its own UI/state, can publish a portable shared revision state, and only uses the optional bridge when hosted by Chatty-EDU.
