# Lesson Studio (Demo)

`Lesson Studio (Demo)` is a teacher-facing sample module for shaping one lesson at a time.

## What it shows

- a portable `manifest.json`
- a hosted browser-style dashboard through `visual_load.json`
- module-owned UI/state that still works outside Chatty-EDU
- an optional bridge handoff so Chatty-EDU can understand the latest lesson status

## Standalone use

Open `web/index.html` in a browser. The module stores its own draft in browser local storage.

## Hosted use

Drop the folder into `chatty-edu/modules/` and open the module from the Modules menu. When hosted, the dashboard stays the same, but Chatty-EDU can read the bridge summary/snapshot on suspend.
