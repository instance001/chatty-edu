# Module Submission Template

Use this when you are handing a module to another person, team, or future-you for review, testing, or inclusion.

This is the "here is the module, here is what it needs, here is how to test it" handoff sheet.

## Copy-and-fill template

```text
# Module Submission: <Module Name>

## Basic info

- Module name: <display name>
- Module ID: <module_id>
- Version: <x.y.z>
- Status: <Ready to share / Private testing only / Experimental>
- Builder: <name or team>
- Date: <YYYY-MM-DD>

## What this module does

<One short paragraph in plain language. What problem does it solve?>

## Included files

- `manifest.json`
- `visual_load.json` <if present>
- `HANDSHAKE.md`
- `README.md` / `USER_MANUAL.md`
- `CHANGELOG.md` <if present>
- release notes <if present>
- any special assets or data folders

## Standalone use

- How to launch:
  - ...
- Prerequisites:
  - ...
- Notes:
  - ...

## Hosted in Chatty-EDU

- How Chatty-EDU should host it:
  - `webview` / `native_window`
- Build step needed before hosting:
  - yes / no
- If yes, command:
  - ...
- Expected launch target:
  - ...

## Workflow-loop fit

- Best role in a larger Chatty-EDU workflow:
  - ...
- Typical handoff into this module:
  - ...
- Typical handoff out of this module:
  - ...
- Companion modules or tools:
  - ...

## Bridge / handoff support

- Bridge included:
  - yes / no
- What the bridge reports:
  - summary
  - snapshot
  - tags / payload <if used>
- Known limits:
  - ...

## Test checklist

- [ ] Launches standalone
- [ ] Core workflow works standalone
- [ ] Opens inside Chatty-EDU
- [ ] Hosted UI loads correctly
- [ ] Tab leave/close gives a useful handoff
- [ ] Docs match actual behavior
- [ ] No secrets or junk files included

## Known issues

- ...
- ...

## Reviewer focus

Please pay extra attention to:

- ...
- ...

## Recommended first test

- ...

## Packaging note

- Zip name:
  - ...
- Extracted folder name:
  - ...

## Links

- README / manual:
  - ...
- Release notes:
  - ...
- Changelog:
  - ...
```

## When to use this

Good times to use it:

- sending a module to another builder for review
- shipping a test build to users
- submitting a module to a shared internal module library
- handing a paused module back to your future self after a while

## Why this helps

It keeps the handoff focused on:

- what the module is
- how to run it
- how to test it
- where it fits in a larger workflow loop
- what still needs attention

That reduces guesswork and keeps reviews calmer and faster.

## Suggested companions

This template pairs well with:

- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md`
- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`
- `chatty-edu/docs/MODULE_RELEASE_NOTES_TEMPLATE.md`
- `chatty-edu/docs/CHANGELOG_TEMPLATE.md`
