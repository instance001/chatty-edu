# Module Review Rubric

Use this when you want a quick, plain-language answer to:

- "Is this module ready to share?"
- "What still needs tightening up?"
- "Would another person be able to install and use this without hand-holding?"

This is meant as a lightweight self-review or teammate-review pass after:

- `chatty-edu/docs/MODULE_TEMPLATE_CHOOSER.md`
- `chatty-edu/docs/MODULE_BUILDER_CHECKLIST.md`
- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md`

## How to use this rubric

Work top to bottom.

For each section, mark one:

- **Pass**: solid, no obvious blocker
- **Needs work**: usable, but rough or unclear
- **Blocker**: should be fixed before sharing

If any section is a blocker, the module is not share-ready yet.

## 1) Standalone behavior

Pass if:

- the module launches by itself
- the main UI works without Chatty-EDU
- save/load or normal workflow still works standalone
- missing Chatty-EDU bridge hooks do not break the app

Blocker examples:

- only works when launched by Chatty-EDU
- crashes if `CHATTYEDU_*` env vars are missing
- module depends on Chatty-EDU-managed files to function at all

## 2) Hosted behavior inside Chatty-EDU

Pass if:

- Chatty-EDU discovers the module correctly
- the module opens from the **Modules** menu
- the real native UI or hosted webview appears in the tab
- closing or switching tabs does not leave the module in a broken state

Needs work examples:

- hosted UI works, but the title/docking is unreliable
- requires a manual build that is not documented clearly

Blocker examples:

- hosted UI never appears
- launch path is wrong
- module only shows a broken or blank surface

## 3) Boundary cleanliness

Pass if:

- Chatty-EDU hosts the module but does not own its real state model
- the bridge is optional and lightweight
- removing the bridge would remove compatibility, not destroy the tool

Blocker examples:

- module stores its core data in Chatty-EDU-only files
- module assumes Chatty-EDU APIs are always present
- module cannot function outside the Chatty-EDU ecosystem anymore

## 4) Bridge and handoff quality

Pass if:

- `summary` is short and actually useful
- `snapshot` adds context without becoming a giant dump
- bridge updates happen on meaningful changes, not constantly
- suspend handoff helps the host app understand what happened

Needs work examples:

- summary is vague like "stuff happened"
- snapshot is too noisy or too thin

Blocker examples:

- bridge data is stale or misleading
- module spams bridge output every frame or action
- leaving the tab gives Chatty-EDU no meaningful context when the bridge is supposed to exist

## 5) Docs and onboarding

Pass if:

- `HANDSHAKE.md` matches what the module really does
- `README.md` or `USER_MANUAL.md` explains install and use clearly
- release notes are clear if this is an update release
- changelog history is understandable if the module is evolving over time
- build steps are explicit if the module needs them
- a new user could understand the module without guessing

Needs work examples:

- docs exist but skip prerequisites
- docs assume prior knowledge the target user may not have

Blocker examples:

- no install instructions
- outdated docs
- docs describe a different workflow than the real module

## 6) Packaging quality

Pass if:

- the shipped folder is clean
- there is one obvious folder to drop into `chatty-edu/modules/`
- junk folders are excluded
- no secrets, logs, or machine-specific files are bundled

Blocker examples:

- release zip extracts flat files instead of a module folder
- includes `target/`, `.venv/`, secrets, or local debris
- package cannot be installed cleanly by another person

## 7) Naming and clarity

Pass if:

- folder name is sensible
- `module_id` is stable
- `display_name` is human-friendly
- visible labels feel consistent and understandable

Needs work examples:

- internal dev names leak into the user-facing UI
- unclear button text

Blocker examples:

- module identity is inconsistent across files
- two names are used for the same thing in confusing ways

## 8) Performance and stability

Pass if:

- normal use does not feel obviously broken
- the module does not crash during basic flows
- hosted mode and standalone mode are both reasonably stable

Needs work examples:

- small hiccups, rough edges, or minor layout bugs

Blocker examples:

- frequent crashes
- obvious lockups
- known broken primary workflow

## 9) User trust and safety

Pass if:

- the module does what it says it does
- risky actions are visible and understandable
- file paths and actions are predictable

Needs work examples:

- confusing save/export behavior
- unclear wording around destructive actions

Blocker examples:

- hidden destructive behavior
- writes outside expected module-owned locations without clear user intent
- misleading status messages

## Quick decision

### Ready to share

Use this if:

- no blockers
- only minor "needs work" items
- another person could likely install and use it successfully

### Ready for private testing only

Use this if:

- no catastrophic issue, but multiple "needs work" sections remain
- you would still need to personally explain setup or recovery steps

### Not ready to share

Use this if:

- any blocker is present

## Suggested review note format

If you want a simple teammate handoff note, use:

```text
Module review: <module name>

Status: Ready to share / Private testing only / Not ready to share

Strengths:
- ...
- ...

Needs work:
- ...
- ...

Blockers:
- ...

Next recommended step:
- ...
```

## The spirit of the rubric

We are not trying to make builders jump through hoops.

We are trying to protect three things:

- portability
- clarity
- trust

If a module stays standalone, hosts cleanly, reports a useful handoff, and can be installed without guesswork, it is in good shape.

For a fuller handoff package when sharing the module, also use:

- `chatty-edu/docs/MODULE_SUBMISSION_TEMPLATE.md`
