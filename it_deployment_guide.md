# Chatty-EDU IT Deployment Guide (v0.5)

Audience: school IT, sysadmins, and deployment teams.

Chatty-EDU is designed to run **offline** on local school hardware. It stores data as files on disk under a configurable base directory. It can also optionally connect to other nearby Chatty-EDU instances over trusted local Wi-Fi or LAN when a user enables the local networking feature.

The small ECG indicator in the GUI is part of that deployment story: it is a visible transparency feature intended to help schools, students, and parents see that Chatty-EDU is actively doing local work instead of hiding background activity.

## Quick start (recommended)
1) Place the `chatty-edu.exe` in a writable folder (portable) **or** plan a dedicated data folder and use `--base-path`.
2) Provision an approved GGUF model into `<base>/models/` (no weights are shipped with Chatty-EDU).
3) Launch GUI: `chatty-edu.exe --mode gui --base-path <base>`
4) On startup, Chatty-EDU auto-scans `<base>/models/` and assigns roles where possible. Use **File -> Models** to inspect or override the selection.
5) Teacher unlock: default PIN is `0000` (intended to be changed immediately).

## Deployment models
### Option A: Portable folder (simplest)
- Put `chatty-edu.exe` in a folder that users can write to (e.g., a USB stick or a writable local folder).
- By default, Chatty-EDU uses `./data` next to the executable.

Pros: minimal setup.  
Cons: data lives beside the EXE; ensure the folder is protected/backed up.

### Option B: Installed binary + separate data (recommended for managed devices)
If you install the EXE under a non-writable location (e.g., `C:\Program Files\...`), you must set a writable base path:

- Per-device base path: `C:\ProgramData\Chatty-EDU\data`
- Per-user base path: `C:\Users\<user>\Chatty-EDU\data`

Example:
`chatty-edu.exe --mode gui --base-path C:\ProgramData\Chatty-EDU\data`

## Command-line flags
- `--mode gui` (default) or `--mode cli`
- `--base-path <path>`: overrides the data directory

Note: `--mode model-worker` is an internal subprocess mode used to isolate the local model; do not run it directly.

## Data directories and backup
All runtime state is stored under the base path (default `./data`):
- `config/` settings, UI state, and Bookkeeper context files
- `config/bookkeeper/` local audit-history logs plus `memory_jogger.txt`
- `homework/outgoing/` teacher-authored pack Markdown (`*.md`)
- `homework/assigned/` homework packs (`homework_pack_*.json`)
- `homework/completed/` student submissions (`submission_*.json`)
- `homework/marking/` marking exports (`marking_*.md`)
- `homework/printables/` student printables (`student_*.md`)
- `homework/rubrics/` teacher rubrics (`rubric_*.md`)
- `revision/notes/` student revision notes and progress (`revision_note_*.json`)
- `revision/past_papers/` imported past papers and teacher revision materials
- `models/` GGUF model files

Backups are file-based: copy the entire base directory to back up or migrate a deployment.

Revision uses completed homework as source material. If you are migrating a deployment and want Revision history to remain intact, back up both `homework/completed/` and `revision/`.

## Drop-in module deployments
Chatty-EDU can now host standalone modules inside tabs without making those modules depend on Chatty-EDU to function.

Practical deployment shape:
- place the module folder under `<base>/modules/` (or the repo-local `modules/` during development)
- preferred portable modules use `manifest.json`
- legacy EDU modules can still use `module.json`
- hosted standalone modules can add `visual_load.json`
- optional bridge files live under `bridge/`

For builder and review docs, see:
- `docs/MODULES.md`
- `docs/MODULE_TEMPLATE_CHOOSER.md`
- `docs/DEMO_MODULES.md`
- `docs/MODULE_PACKAGING_GUIDE.md`
- `module_templates/`

Bundled EDU demo modules are included under `modules/demo_*` and can be left in place as references or removed for a leaner deployment image.

## Optional local networking
Chatty-EDU now includes an optional local peer mode for nearby EDU instances.

What it is for:
- discovering other Chatty-EDU machines on the same trusted local network
- connecting EDU-to-EDU over the LAN
- sharing lightweight presence and short handoff notes
- sending homework packs, revision packs, and classroom setup bundles across the local network

What it is not:
- internet sync
- cloud messaging
- remote admin tooling

Operational notes:
- local discovery uses UDP broadcast on port `45841`
- connected sessions use a dynamically chosen local TCP listener port
- the feature is off by default until a user enables `Make available for connectivity`
- local firewall policy may need to allow Chatty-EDU on trusted local networks
- Chatty-EDU and Chatty-Cog use different local networking identifiers, so they do not accidentally cross-connect
- per-device aliases and group labels are local UI preferences, not a central directory or identity system
- received transfer inboxes live under `network_inbox/` and are intended to be previewed before apply
- `workflow_bundles/` are for lesson-wide setup only, not secret material or student identity data

Recommended deployment stance:
- leave networking off on single-device student setups
- enable it only where local peer collaboration or handoff is useful
- treat it as a local convenience feature, not a hardened trust boundary

For a plain-language explanation suitable for staff, see `docs/NETWORKING.md`.

## Models (GGUF) provisioning
- Place approved GGUF files under `<base>/models/`.
- On startup, the largest valid GGUF is auto-assigned to the main AI role.
- If 2 or more valid GGUFs are present, the smallest valid GGUF is auto-assigned to the Bookkeeper role.
- If only 1 model is present, the main AI uses it and Bookkeeper falls back to keyword-only summary mode.
- If no model is present, the app stays friendly and prompts the user to drop a GGUF into `<base>/models/` to get started.
- You can still inspect or override the selection in the GUI via **File -> Models**.
- Chatty-EDU runs the model in an internal worker process; incompatible GGUF files should fail with an error instead of crashing the app.

Operational tips:
- Prefer smaller GGUFs on low-RAM machines.
- Treat model weights as software assets: track licensing, source, and version like any other third-party dependency.

## Teacher controls and device posture
Teacher controls are gated by a local PIN/secret stored in `config/settings.json`.
- Default PIN: `0000` (change immediately).
- This is **not** a strong security boundary: anyone with file access can edit settings.

For student-facing deployments, use OS controls:
- Separate Windows accounts (teacher vs student) and restrict access to the base path.
- Use disk encryption and standard endpoint protections.
- Consider kiosk/assigned-access style configurations where appropriate.

## Transparency and trust in deployment
- The ECG widget is intentionally visible in the UI so people in the room can see when Chatty-EDU is active.
- It is not a replacement for firewall or endpoint controls, but it supports trust in the zero-calls-home design by avoiding a "silent black box" feel.
- For family-facing or school-review deployments, it is reasonable to explain the ECG widget as a local transparency cue: model work causes visible activity, while Chatty-EDU itself still operates without cloud endpoints in normal use.

## Printing / paper workflows
Teachers can export:
- Student printables to `<base>/homework/printables/`
- Teacher rubrics/marking guides to `<base>/homework/rubrics/`

These exports are Markdown so schools can convert to PDF/print using their standard tooling.

## Building from source (optional)
Prereqs:
- Rust toolchain.

Local model builds (default feature) compile llama.cpp via CMake, so you need CMake + a C/C++ toolchain.
- Build without local model support: `cargo build --no-default-features`
- Build with local model support: `cargo build`

## Troubleshooting
- Health check / support: use **File -> Copy Diagnostic report** (or open the Diagnostics tab) and paste the report into your support request.
- Base path not writable: launch with `--base-path` pointing to a writable directory.
- "Model worker exited immediately" / `GGML_ASSERT`: the GGUF is likely incompatible with the embedded llama.cpp bindings; try a different GGUF/quant.
- No model selected on first launch: confirm `<base>/models/` contains at least one valid GGUF, then refresh or relaunch. Single-model deployments are valid; Bookkeeper will simply stay in keyword-only mode.
- No packs visible: confirm the correct `--base-path` and that packs are in `<base>/homework/assigned/` (JSON) or `<base>/homework/outgoing/` (Markdown, then transcribe).
- No EDU peers found in Networking: make sure both devices are on the same trusted local network, at least one device has `Make available for connectivity` enabled, and local firewall policy allows Chatty-EDU discovery and local peer traffic.
