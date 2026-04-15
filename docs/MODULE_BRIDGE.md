# Module Bridge (Portable Status Plug)

Use the bridge when you want a module to:

- keep its own standalone UI and state
- run normally outside Chatty-EDU
- optionally report a short status + snapshot back to Chatty-EDU when hosted in a tab

This is the simple compatibility rule:

- `manifest.json` makes the module discoverable
- `visual_load.json` lets Chatty-EDU host the real UI
- `bridge/status.json` lets the module report back what happened
- `bridge/shared_room_state.json` lets room-aware modules see the current classroom-room policy when hosted
- `bridge/shared_room_events.json` lets a hosted module read recent low-latency classroom/session events
- `bridge/outgoing_room_events.json` lets a hosted module emit lightweight room/session events back into the LAN room
- `bridge/incoming_assets/<lane_id>/` lets Chatty-EDU drop approved files or binary/text payloads into declared module inbox lanes
- `bridge/log_sources.json` lets the module declare recent module-local logs Chatty-EDU may surface

If you remove the bridge logic, the module still works standalone. It just stops reporting back to Chatty-EDU.

Quick helpers:

- `chatty-edu/docs/MODULE_BRIDGE_SNIPPETS.md`
- `chatty-edu/docs/MODULE_BRIDGE_HELPER_RUST.rs`
- `chatty-edu/docs/MODULE_BRIDGE_HELPER_WEBVIEW.js`

## File contract

Runtime files:

```text
your_module/
  bridge/
    status.json
    shared_room_state.json
    shared_room_events.json
    outgoing_room_events.json
    incoming_assets/
      lesson_assets/
        <asset metadata>.json
        <payload file>
    log_sources.json
```

Chatty-EDU reads these files only when the hosted module chooses to provide them.

## `status.json` schema

```json
{
  "module_id": "study_lab",
  "event_type": "suspend_rundown",
  "summary": "Study Lab is halfway through a revision plan. Next step: convert the current draft into a printable worksheet.",
  "snapshot": "# Study Lab Snapshot

- Current topic: algebra
- Progress: 50%",
  "tags": ["study", "revision", "webview"],
  "payload": {
    "topic": "algebra",
    "progress": 0.5
  },
  "updated_at_unix_ms": 1774588800000
}
```

Fields:

- `module_id` - should match `manifest.json`
- `event_type` - usually `suspend_rundown`
- `summary` - short human handoff text
- `snapshot` - optional longer state dump or preview
- `tags` - optional search/filter tags
- `payload` - optional free-form JSON object
- `updated_at_unix_ms` - optional Unix milliseconds timestamp

## Optional `log_sources.json`

Use this when the module already has its own logging system and you want Chatty-EDU to surface recent declared log tails in the bridge panel.

Example:

```json
{
  "sources": [
    {
      "path": "logs/session.log",
      "label": "Session Log",
      "format": "log",
      "tail_lines": 80,
      "tail_chars": 4000
    }
  ]
}
```

Safety rules:

- Chatty-EDU only reads paths the module explicitly declares
- paths must remain inside the module folder
- absolute paths and `..` traversal are ignored
- missing logs are skipped quietly

Starter file:

- `chatty-edu/docs/MODULE_LOG_SOURCES_TEMPLATE.json`

## Native hosted modules

When Chatty-EDU launches a hosted native-window module, it sets:

- `CHATTYEDU_HOSTED=1`
- `CHATTYEDU_MODULE_DIR=<absolute module folder>`
- `CHATTYEDU_BRIDGE_DIR=<absolute bridge folder>`
- `CHATTYEDU_BRIDGE_STATUS=<absolute path to bridge/status.json>`
- `CHATTYEDU_BRIDGE_SHARED_ROOM_STATE=<absolute path to bridge/shared_room_state.json>`
- `CHATTYEDU_BRIDGE_SHARED_ROOM_EVENTS=<absolute path to bridge/shared_room_events.json>`
- `CHATTYEDU_BRIDGE_OUTGOING_ROOM_EVENTS=<absolute path to bridge/outgoing_room_events.json>`
- `CHATTYEDU_BRIDGE_INCOMING_ASSETS_DIR=<absolute path to bridge/incoming_assets>`
- `CHATTYEDU_BRIDGE_LOG_SOURCES=<absolute path to bridge/log_sources.json>`

Recommended pattern:

1. read `CHATTYEDU_BRIDGE_STATUS`
2. if present, write the JSON status file there
3. optionally write or ship `log_sources.json`
4. if absent, do nothing special

That keeps the module portable.

## Hosted webview modules

When Chatty-EDU hosts a `webview` module, it injects:

```js
window.chattyEduBridge.available
window.chattyEduBridge.updateStatus(payload)
window.chattyEduBridge.clearStatus()
window.chattyEduBridge.readSharedRoomState()
window.chattyEduBridge.readSharedRoomEvents()
window.chattyEduBridge.readIncomingAssets(laneId)
window.chattyEduBridge.incomingAssetUrl(laneId, payloadFileName)
window.chattyEduBridge.consumeIncomingAsset(laneId, assetId)
window.chattyEduBridge.emitRoomEvent(payload)
```

Recommended pattern:

```js
if (window.chattyEduBridge?.available) {
  window.chattyEduBridge.updateStatus({
    module_id: "your_module",
    event_type: "suspend_rundown",
    summary: "Short handoff text here.",
    snapshot: "Longer snapshot here.",
    tags: ["study", "webview"],
    payload: { progress: 0.75 }
  });
}
```

If the module runs in a normal browser outside Chatty-EDU, `window.chattyEduBridge` simply does not exist and the module still works.

## Optional `incoming_assets/` bridge inbox

This is the portable classroom asset lane for modules that need more than tiny room events or JSON state.

Use it when the module wants to receive things like:

- lesson handouts or companion files
- small binary assets
- exported class presets
- rich markdown / JSON / CSV payloads
- module-specific classroom packs that should land in the module folder, not the global networking inbox

Chatty-EDU only auto-delivers a received transfer into a module lane when:

- the transfer is already scoped to that module
- exactly one declared incoming `asset_lanes[]` entry matches it
- that lane uses `delivery_mode: "bridge_inbox"`

Otherwise the transfer stays in the normal networking inbox until the teacher or user chooses a lane manually.

Each delivered asset creates:

- a small metadata JSON record in `bridge/incoming_assets/<lane_id>/`
- the original payload beside it

Hosted webviews can use:

- `readIncomingAssets(laneId)` to list waiting assets
- `incomingAssetUrl(laneId, payloadFileName)` to read the payload
- `consumeIncomingAsset(laneId, assetId)` to remove it after the module has imported or applied it

That keeps the boundary clean:

- Chatty-EDU fills only declared inbox lanes
- the module decides when and how to import the payload
- removing the bridge plug removes compatibility without breaking the standalone tool

## Optional `shared_room_events.json` and `outgoing_room_events.json`

These files are for modules that want a lightweight event lane in addition to the heavier shared-state lane.

Use them for things like:

- tiny multiplayer or lesson-room moves
- ready / waiting states
- teacher nudges such as "next round starting"
- short classroom tool signals that should not become full inbox artifacts

Recommended shape for outgoing events:

```json
{
  "events": [
    {
      "event_type": "ready_state",
      "label": "Student ready",
      "content_type": "application/json",
      "payload_text": "{\"ready\":true}"
    }
  ]
}
```

Behavior:

- the hosted module writes or appends lightweight items to `outgoing_room_events.json`
- Chatty-EDU relays them across the current classroom room/session when that module is active in the room lane
- recent incoming events are mirrored back into `shared_room_events.json`
- this lane is intentionally for **small, low-latency text payloads**, not big files or full setup bundles

## Optional `shared_room_state.json`

This file is for modules that explicitly declare `room_aware` or `multiplayer` in `network_capabilities.json`.

Chatty-EDU writes it for hosted modules so they can react to the classroom-room policy without handing over their real runtime to EDU.

Use it for things like:

- showing whether the current lesson room is focused on this module
- reflecting teacher override inside the module UI
- surfacing talking-stick ownership in multiplayer or turn-based lesson tools
- adapting module behavior when the room policy limits AI use

It can also carry an optional host-authoritative session layer for lesson-room or multiplayer modules:

- `session_active`
- `session_id`
- `session_revision`
- `session_label`
- `host_authoritative`
- `participants[]`

That gives a hosted module a portable way to understand "there is an active lesson session, this is its revision, these are the current participants, and the teacher/host is authoritative" without handing the module's real runtime over to Chatty-EDU.

## Best practices

- keep `summary` to one short paragraph
- put richer details into `snapshot`
- if the module already has useful logs, declare them in `log_sources.json` instead of asking the user to type a handoff
- update the bridge only when state changes meaningfully
- treat the bridge as an optional compatibility layer, not your main database

## What Chatty-EDU does with it today

Today, Chatty-EDU uses the bridge to:

- surface module-reported status in the hosted tab
- show the latest snapshot if the module provides one
- surface recent declared module-local log excerpts in the bridge panel

That gives EDU users a clean hosted-module experience without making Chatty-EDU the module's runtime brain.

## Shipped examples

- `chatty-edu/modules/demo_lesson_studio/web/app.js` - room-aware + host-authoritative lesson-session example
- `chatty-edu/modules/demo_revision_sprint/web/app.js`
- `chatty-edu/modules/demo_teacher_notebook/src/main.py`
