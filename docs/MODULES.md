# Chatty-EDU Modules (Standalone Plug-In System)

This document describes how to build drop-in modules for `chatty-edu`.

Important boundary:

- Chatty-EDU modules are for the education-specific Chatty-EDU runtime
- ChattyCog modules are for the broader general-purpose ChattyCog runtime
- those ecosystems are intentionally kept separate and should not be treated as interchangeable hosts
- that separation is deliberate for safety and policy reasons, especially around schools, classrooms, and kid-facing use

## What a module is

A module is a folder dropped into `chatty-edu/modules/`.

Chatty-EDU supports two discovery styles:

1. **Preferred portable style**
   - `manifest.json`
   - optional `visual_load.json`
   - optional `bridge/status.json`
   - optional `bridge/log_sources.json`

2. **Legacy EDU style**
   - `module.json`
   - used by older built-in EDU panels and markdown modules

Starter templates live in:

- `chatty-edu/module_templates/`

That location is intentional so templates do not appear as live modules until you copy them into `chatty-edu/modules/`.

If you are choosing a starter or getting a module ready to share, also use:

- `chatty-edu/docs/MODULE_TEMPLATE_CHOOSER.md`
- `chatty-edu/docs/MODULE_BUILDER_CHECKLIST.md`
- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md`
- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`
- `chatty-edu/docs/MODULE_RELEASE_NOTES_TEMPLATE.md`
- `chatty-edu/docs/CHANGELOG_TEMPLATE.md`
- `chatty-edu/docs/MODULE_SUBMISSION_TEMPLATE.md`

## Recommended folder layout

```text
chatty-edu/
  modules/
    your_module/
      manifest.json
      visual_load.json         (optional)
      network_capabilities.json (optional)
      bridge/status.json       (optional runtime file)
      bridge/incoming_assets/  (optional runtime lane; approved classroom files/payloads land here)
      bridge/log_sources.json  (optional runtime file)
      HANDSHAKE.md             (recommended)
      README.md                (recommended)
      STATE_TEMPLATE.md        (optional fallback notes)
      ...your real app files...
```

Only the manifest is required for discovery in the preferred flow.

## Preferred `manifest.json` schema

Minimum example:

```json
{
  "module_id": "study_lab",
  "display_name": "Study Lab",
  "icon": "book",
  "description": "A standalone study workflow module for Chatty-EDU."
}
```

Supported fields:

- `module_id` - required stable identifier
- `display_name` - required tab/menu name
- `icon` - optional text/icon hint
- `description` - optional short human summary
- `author` - optional
- `version` - optional
- `roles` - optional list like `teacher` / `student`
- `permissions` - optional descriptive list
- `order` - optional sort hint
- `visual_load` - optional inline hosted-UI block (a separate `visual_load.json` is cleaner)
- `network_capabilities` - optional inline declaration of which network lanes the module intentionally supports (a separate `network_capabilities.json` is cleaner)

Notes:

- `module_id` and `display_name` must be non-empty or the module is ignored.
- Unknown fields are ignored.
- If a module uses `manifest.json`, Chatty-EDU will also load `visual_load.json` when present.

## Legacy `module.json` support

Chatty-EDU still supports older module manifests with an `entry` block.

Supported legacy entry types:

- `builtin_panel`
- `markdown`
- `static_html`
- `external_process`

Current legacy behavior:

- `builtin_panel` renders an in-app EDU panel
- `markdown` renders the markdown file in the tab
- `static_html` is recognized as a declared surface but is not automatically hosted unless you add `visual_load.json`
- `external_process` remains gated by safe mode and is not the recommended path for new modules

For new module work, prefer:

- `manifest.json`
- `visual_load.json`
- optional portable bridge files

## Discovery rules

Chatty-EDU scans only one folder level under:

- `chatty-edu/modules/*/manifest.json`
- `chatty-edu/modules/*/module.json`
- `chatty-edu/modules/*/visual_load.json` (optional companion file)
- `chatty-edu/modules/*/network_capabilities.json` (optional companion file)

It does not recurse deeper than one directory for discovery.

If a manifest fails to parse, the module is skipped and the app keeps going.

## What users see in a module tab

In practice, modules show up in one of three ways:

### 1) Docked native app

Use this when the module already has its own desktop window.

Chatty-EDU launches that real app and docks that real standalone window into the tab.

### 2) Docked web dashboard

Use this when the module already has its own HTML/CSS/JS interface.

Chatty-EDU hosts that real browser-style dashboard in a bundled webview helper.

### 3) EDU fallback surface

Use this when the module is older, headless, markdown-based, or simply does not advertise a hosted UI.

In that case Chatty-EDU falls back to the legacy `module.json` entry behavior.

## Why the split exists

The point is to keep modules portable and simple:

- desktop apps keep their real desktop UI
- browser-style tools keep their real browser UI
- older or headless EDU modules still remain usable

That way:

- builders who already have a UI do not need to rebuild it for Chatty-EDU
- builders with no GUI can still make useful EDU modules
- removing the EDU compatibility plug does not break the standalone tool itself

## Compound workflow design goal

The larger goal is not only to host a lesson tool inside a tab. It is to let specialized modules participate in a compounding in-app workflow.

That can look like:

- the main Chatty-EDU AI helps sketch a lesson, revision pack, or classroom activity
- a specialist module turns that draft into structured templates or working materials
- another hosted tool reviews media, worksheets, or visual examples
- the next module adds prompt, rubric, or training guidance
- the teacher or learner keeps iterating without dropping out to the desktop between steps

Inside that loop, each lane has a job:

- hosted UI keeps the real specialist tool visible
- fallback notes or local workspace hold immediate working state
- bridge files surface status, logs, and approved assets back into the EDU shell
- room-aware or asset-lane features let classroom-oriented modules take part without losing portability

This matters for pure EDU flows and for wider Chatty ecosystem loops as well. A builder might draft content in an EDU module, refine media in a sibling tool, then bring the result back as a lesson asset. The module system is meant to support that kind of flywheel, not just isolated panels.

Keep the boundary explicit while you do it:

- build for Chatty-EDU when the tool belongs in the school-safe education ecosystem
- build for ChattyCog when the tool belongs in the broader general-purpose ecosystem
- do not assume one packaged module should freely move between both hosts without an intentional separate adaptation and review pass

## Hosted visual load-in

If a module ships `visual_load.json`, Chatty-EDU can host the real standalone UI directly in the tab.

See:

- `chatty-edu/docs/MODULE_VISUAL_LOAD.md`
- `chatty-edu/docs/MODULE_VISUAL_LOAD_TEMPLATE.json`
- `chatty-edu/docs/MODULE_VISUAL_LOAD_WEBVIEW_TEMPLATE.json`

## Portable bridge

If a hosted standalone module wants to report back what happened, it can optionally write:

- `bridge/status.json`
- `bridge/log_sources.json`

Today, Chatty-EDU surfaces that information in the module's **Chatty-EDU bridge** panel so the user can inspect module-reported status and any declared recent log excerpts.

That keeps the boundary clean:

- module owns its real runtime, UI, and state
- Chatty-EDU hosts the module and reads the optional handoff plug
- remove the plug and the module still works standalone

See:

- `chatty-edu/docs/MODULE_BRIDGE.md`
- `chatty-edu/docs/MODULE_BRIDGE_SNIPPETS.md`
- `chatty-edu/docs/MODULE_BRIDGE_TEMPLATE.json`
- `chatty-edu/docs/MODULE_LOG_SOURCES_TEMPLATE.json`
- `chatty-edu/docs/MODULE_NETWORK_CAPABILITIES_TEMPLATE.json`

## Network capability manifest

If your module participates in classroom or LAN sharing, add:

- `chatty-edu/modules/<module>/network_capabilities.json`

Example:

```json
{
  "features": [
    "shared_state_publish",
    "shared_state_receive",
    "host_authoritative"
  ],
  "notes": [
    "Teacher can publish lesson-ready state for connected learners.",
    "Incoming mirrored state is applied through the portable EDU bridge."
  ]
}
```

Recognized features today:

- `shared_state_publish`
- `shared_state_receive`
- `workflow_bundle_send`
- `workflow_bundle_receive`
- `pack_send`
- `pack_receive`
- `lukewarm_context_publish`
- `lukewarm_context_receive`
- `room_aware`
- `multiplayer`
- `host_authoritative`

Optional `asset_lanes` let a module declare specific bridge inboxes for richer classroom payloads.

Example:

```json
{
  "features": ["shared_state_receive", "host_authoritative"],
  "asset_lanes": [
    {
      "lane_id": "lesson_assets",
      "label": "Lesson Assets",
      "direction": "incoming",
      "delivery_mode": "bridge_inbox",
      "artifact_kinds": ["module_asset_file", "pack_file"],
      "accepted_content_types": ["text/markdown", "application/json", "application/octet-stream"],
      "max_bytes": 8388608,
      "replayable": true
    }
  ]
}
```

Use asset lanes when the module wants Chatty-EDU to hand it richer files or payloads through `bridge/incoming_assets/<lane_id>/` while still letting the standalone module decide when and how to import them.

`host_authoritative` matters most when a module also uses `room_aware` or `multiplayer`. It tells Chatty-EDU that lesson-room sessions for that module should behave as "teacher/host leads, learners follow revisions" rather than as an unstructured shared room.

Why this exists:

- it keeps classroom networking explicit instead of guessed
- it lets Chatty-EDU disable or warn on actions the module has not intentionally declared
- it keeps standalone modules portable, because removing the file removes EDU compatibility without breaking the tool itself

## Human-facing module contract

It is strongly recommended that modules also ship:

- `HANDSHAKE.md`

This is the human-readable contract for what the module is for, what it expects, and how it should hand work back.

Starter:

- `chatty-edu/docs/MODULE_HANDSHAKE_TEMPLATE.md`

## Starter templates

Chatty-EDU ships three starter templates for builders:

- `chatty-edu/module_templates/template_module/` - hosted webview starter
- `chatty-edu/module_templates/template_native_rust_module/` - native Rust starter
- `chatty-edu/module_templates/template_python_module/` - native Python starter

Use `chatty-edu/docs/MODULE_TEMPLATE_CHOOSER.md` if you are unsure which one fits best.

## Living demo references

Bundled EDU-flavoured demo modules live under `chatty-edu/modules/demo_*`.

- `demo_lesson_studio/` shows a teacher-facing hosted webview
- `demo_revision_sprint/` shows a student-friendly hosted webview
- `demo_teacher_notebook/` shows a hosted native Python window plus bridge log sources

Use them as copyable references when you want a concrete example alongside the starter templates.
