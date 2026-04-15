# Lesson Studio (Demo) Handshake

- Module type: hosted webview demo
- Primary state: browser local storage
- Optional bridge: `bridge/status.json`
- Optional shared lesson state: `bridge/shared_state.json`
- Optional incoming mirrored lesson state: `bridge/incoming_shared_state.json`
- Hosted by Chatty-EDU through `visual_load.json`

When hosted, the dashboard calls the Chatty-EDU bridge helper to publish a short summary, a longer snapshot, and a portable shared lesson state. If mirrored state arrives from the network, the module can apply it without giving up its own UI/runtime. If the bridge is unavailable, the module still works normally in a browser.
