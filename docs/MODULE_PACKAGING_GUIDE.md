# Module Packaging Guide

Use this guide when a module is ready to move from "works on my machine" to "safe to hand to another person."

For a simple go/no-go quality pass before shipping, also use:

- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`
- `chatty-edu/docs/MODULE_SUBMISSION_TEMPLATE.md`

Goal:

- keep the module portable
- keep Chatty-EDU compatibility optional
- make install and update steps obvious for other users

## Packaging rule of thumb

Ship the module as if Chatty-EDU does not exist.

Then add the Chatty-EDU plug files as a thin compatibility layer:

- `manifest.json`
- `visual_load.json` (if needed)
- `HANDSHAKE.md`
- optional `bridge/` support

If removing those plug files would break the module itself, the module is too tightly coupled.

## Folder shape to ship

Recommended packaged shape:

```text
your_module/
  manifest.json
  visual_load.json          (optional)
  HANDSHAKE.md
  USER_MANUAL.md            (recommended)
  README.md                 (recommended)
  bridge/
  web/ or src/ or app files
  assets/
  ...module-owned files...
```

Good practice:

- keep launch paths relative to the module folder
- keep assets inside the module folder
- keep module-specific state paths obvious
- include a zero-knowledge manual if the module is more than tiny

## What to include

Usually include:

- the runnable app files
- required assets
- `manifest.json`
- `HANDSHAKE.md`
- `visual_load.json` if the module has a hosted UI path
- `README.md` or `USER_MANUAL.md`
- any small sample data the module needs to demonstrate itself

## What not to include

Usually exclude:

- `target/`
- `node_modules/`
- `.venv/`
- `__pycache__/`
- `.pytest_cache/`
- editor folders like `.vscode/` or `.idea/`
- local logs
- local crash dumps
- generated bridge runtime files like `bridge/status.json`
- personal secrets, API keys, tokens, cookies, or machine-specific config

Rule:

- ship source, assets, and intentional release files
- do not ship personal or machine-specific debris

## Versioning

Keep versioning simple and visible.

Recommended:

- add a `version` field inside `manifest.json` if your module uses one
- mention the current version in `README.md`
- keep a short `CHANGELOG.md` if the module will be updated over time

Simple version format:

- `0.1.0` for early testing
- `0.5.0` for feature-complete beta
- `1.0.0` for first stable release

You do not need fancy release engineering to be helpful. Clear beats clever.

## Release zip structure

Best practice:

- zip the module folder itself
- when extracted, the user should get exactly one folder they can drop into `chatty-edu/modules/`

Good:

```text
demo_research_lab.zip
  demo_research_lab/
    manifest.json
    HANDSHAKE.md
    ...
```

Avoid:

```text
demo_research_lab.zip
  manifest.json
  HANDSHAKE.md
  ...
```

That flat shape makes installation messy.

## Install path for users

Your package should support this simple flow:

1. Download the zip
2. Extract it
3. Copy the extracted module folder into `chatty-edu/modules/`
4. In Chatty-EDU, use **Modules -> Rescan modules**
5. Open the module from the **Modules** menu

If the module also runs standalone, explain that too:

- where to launch it from
- whether it needs a one-time build step
- whether Python, Rust, or Node is required

## If the module needs a build step

Be explicit.

Include:

- exact command
- working folder
- any prerequisites
- where the built output ends up

Examples:

- `cargo build --release`
- `npm install && npm run build`
- `py -3 -m pip install -r requirements.txt`

If Chatty-EDU hosting depends on a built executable, say so clearly in `README.md` and `USER_MANUAL.md`.

## Standalone and hosted compatibility check

Before shipping, test both:

### Standalone

- launch the module directly
- confirm it works without Chatty-EDU
- confirm missing bridge env vars do not break it

### Hosted in Chatty-EDU

- copy the folder into `chatty-edu/modules/`
- rescan modules
- open the module in a tab
- confirm the native UI or webview loads
- use the module
- switch away from the tab
- confirm Chatty-EDU gets a meaningful handoff if the bridge is enabled

## Bridge packaging rule

The bridge is runtime glue, not your product database.

Good packaging behavior:

- include the `bridge/` folder if your app expects it
- do not include stale `bridge/status.json` from your own machine
- let the app recreate bridge files at runtime as needed

## Release checklist

Before shipping:

- folder name is clean and final
- `module_id` is stable
- `display_name` is human-friendly
- launch and build paths are correct
- hosted UI still works
- standalone launch still works
- `HANDSHAKE.md` matches the real module behavior
- `README.md` or `USER_MANUAL.md` explains install and use
- no personal data or secrets are left in the folder
- no giant build or output folders are accidentally bundled

## Recommended extra files

Helpful but optional:

- `README.md`
- `USER_MANUAL.md`
- `CHANGELOG.md` using `chatty-edu/docs/CHANGELOG_TEMPLATE.md`
- release notes using `chatty-edu/docs/MODULE_RELEASE_NOTES_TEMPLATE.md`
- `LICENSE` later, when you are ready
- sample screenshots

## A good packaged module feels like this

A strong release should let another person:

- understand what the module does
- install it without guessing
- run it standalone if they want
- drop it into Chatty-EDU if they want
- remove the Chatty-EDU plug later without destroying the module itself

That is the boundary we want to protect.
