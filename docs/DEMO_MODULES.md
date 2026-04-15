# Chatty-EDU Demo Modules

These demo modules ship with `chatty-edu` so educators and builders can inspect working examples instead of starting from a blank folder.

## Included demos

### `demo_lesson_studio`

- Display: hosted webview
- Roles: teacher
- Purpose: sketch a lesson arc, checks for understanding, resources, and a clean handoff
- Good reference for: browser-style dashboards that still stay standalone outside Chatty-EDU, teacher-led room/session mirroring through `shared_room_state.json`, and a living `lesson_assets` inbox that previews, imports, and consumes incoming module assets

### `demo_revision_sprint`

- Display: hosted webview
- Roles: teacher, student
- Purpose: plan a short revision run with target topics and revision blocks
- Good reference for: student-friendly hosted modules and portable bridge snapshots

### `demo_teacher_notebook`

- Display: hosted native Python window
- Roles: teacher
- Purpose: capture observations, support moves, and follow-up checkpoints
- Good reference for: native-window hosting plus `bridge/log_sources.json` and module-owned logs

## Why these demos exist

- They show the hosted module system in real EDU-flavoured use cases.
- They stay portable: each demo still runs as its own tool outside Chatty-EDU.
- They show different plug patterns without forcing module builders into one UI stack.
- `Lesson Studio` is the living example for host-authoritative lesson-room behavior in a hosted module.
- `Lesson Studio` is also the living example for consuming incoming lesson assets from a declared bridge asset lane inside the module UI itself.

## Where to look next

- Module system overview: `MODULES.md`
- Hosted visual loading: `MODULE_VISUAL_LOAD.md`
- Portable bridge contract: `MODULE_BRIDGE.md`
- Builder chooser: `MODULE_TEMPLATE_CHOOSER.md`
- Starter templates: `../module_templates/`
