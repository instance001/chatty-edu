# Chatty-EDU Teacher Manual (v0.5)

Audience: teachers and school IT. Everything runs offline by default; no accounts or cloud calls. Optional local Wi-Fi or LAN connectivity between nearby Chatty-EDU instances is available if you choose to turn it on.

## What you need
- Windows PC (first target).
- Rust toolchain (`https://rustup.rs`). Local model builds (default) also require CMake and a C or C++ toolchain. To build without the local model backend: `cargo run --no-default-features -- --mode gui`.
- Optional: USB for portable data (`--base-path <USB path>`).

## First run (GUI, recommended)
1. Build or run: `cargo run -- --mode gui` (add `--base-path ...` for a USB path).
2. Models (offline AI):
   - Bring your own GGUF model; none is included in this repo.
   - Drop any GGUF into `data/models/`. On startup, Chatty-EDU auto-scans that folder and assigns the largest valid GGUF to the main AI role.
   - If 2 or more valid GGUFs are present, the smallest detected model is assigned to the Bookkeeper role. If only 1 model is present, Bookkeeper stays in keyword-only summary mode.
   - You can still override or inspect roles via File -> Models.
   - Incompatible GGUFs now fail with an error instead of hard-crashing the app; very large models may still be slow or exceed RAM.
   - Model guidance and licensing notes live in `resources/models/` (for example `resources/models/qwen/README.md`).
3. Teacher lock:
   - Default PIN is `0000`. Teacher menu -> unlock with PIN, or use the secret answer if you have set one.
   - While unlocked, change the PIN, set the secret question or answer, adjust game settings, and configure hints-only mode. Lock when done.
4. Import or build packs:
   - Option A (Markdown-first): put a pack `.md` into `data/homework/outgoing/` or generate it in the Teacher Dashboard, then click "Transcribe outgoing (.md -> .json)".
   - Option B (JSON): import a pack `.json` into `data/homework/assigned/` via the Home tab or Teacher menu.
   - Home tab -> "Import pack file" copies a pack into `data/homework/assigned/`.
   - Sample pack: `resources/homework_pack_sample_bundle.json`. If you use the sample attachment, copy `resources/attachments/` alongside the pack.
   - Or use the Pack builder to create and export a pack.
5. Review and tutor:
   - Home tab plus Homework Dashboard: assignments, submissions, metrics, and teacher summary tools.
   - Home now shows the selected assignment, printable or worksheet content, text attachment previews when possible, a submission area, and a compact mirror of the live chat.
   - Home and Main Chat are homework-aware. If a student asks or rephrases an active homework question there, the app intercepts it before generation and pushes the model into a Socratic hint-only response.
   - Chat now includes two student-facing memory sidebars: `Chatty's thoughts` for current-session context on the left and `Memory jogger` for recent cross-session reminders on the right.
   - Revision is now a separate workflow from live homework. It pulls from completed homework, stores notes or progress under `revision/notes/`, and can include past papers under `revision/past_papers/`.
   - The Homework Dashboard now includes separate Revision teacher tools for opening Revision, creating revision packs, and importing past papers.
   - Submissions are written to `data/homework/completed/` and include a hash-chained event log (`start`, `answer`, `hint`, `retry`, `finalize`) plus a `final_hash` for tamper-evidence.
   - Sandbox: the permanent `Sandbox` tab gives Chatty-EDU a real local working area under `data/Chatty_Sandbox/` with a scratchpad, structured task ledger, and approval-gated file actions for longer multi-step work.
6. Watch the ECG window:
   - A small ECG indicator appears in the tab chrome.
   - It samples local Windows hardware activity roughly every 1.5 seconds and is intended as a quick "is the machine busy?" signal, not grading or network telemetry.
   - It is also a transparency and trust feature: teachers, schools, students, and parents can see that Chatty-EDU is visibly doing local work instead of hiding background activity behind a blank screen.
7. Optional local networking:
   - Open the `Network` menu or `Networking` tab if you want one Chatty-EDU machine to discover or connect to another nearby Chatty-EDU machine.
   - Turn on `Make available for connectivity` on one device, then `Refresh discovery` on the other.
   - This is local Wi-Fi or LAN only. It is not cloud sync.
   - Once connected, you can use separate transfer lanes for `Push Pack`, `Push Revision`, and `Push Setup`.

## Homework basics
- Packs are authored in Markdown (`*.md`) under `homework/outgoing/` and transcribed into JSON packs (`homework_pack_*.json`) under `homework/assigned/`, or you can import JSON directly.
- Sharing: homework packs are simple files. You can share a pack `.md` or `.json` with other teachers or schools, including any referenced attachments.
- Optional per-assignment sections in pack Markdown:
  - `### Student Printable` for a paper-friendly handout or visible worksheet text
  - `### Rubric` or `### Marking Guide` for teacher marking guidance
- Use `year_level` as the canonical metadata key in Markdown pack files. Import and transcribe also accept older variants such as `year`, `year level`, `grade`, `grade level`, and `year group`.
- If a question refers to a worksheet, list, table, chart, or handout, place that material in `### Student Printable` or `attachments:` so students can actually see it in-app.
- Students: select assignment, fill "Submit work", attach files if allowed, then "Export submission file" to create `submission_<assignment_id>_<student>.json` in `homework/completed/`.
- Collect student submissions and place them in your `homework/completed/`, then click "Rescan packs + submissions".
- Paper exports: in the Teacher Dashboard, click "Export student printables (.md)" and/or "Export teacher rubrics (.md)" to generate Markdown in `homework/printables/` and `homework/rubrics/`.
- Marking: in the Teacher Dashboard, click "Convert submissions (.json -> .md)" to export marking sheets into `homework/marking/`.
- Duplicate assignment IDs are deduplicated on display, but you should still keep assignment IDs unique inside a pack.

## Revision basics
- Revision is separate from live homework. It uses completed submissions from `homework/completed/` plus any imported past papers in `revision/past_papers/`.
- Students can save their own revision notes or progress under `revision/notes/`.
- The Revision helper can explain more openly than live homework help because the work has already been submitted, but it is still designed to teach rather than just dump answers.
- Student-facing Revision hides teacher-side scores and diagnostic labels, even when those signals exist in the underlying submission data.
- Active homework guardrails still apply on Home and in Chat while a live assignment is selected.

## Shared classroom setup bundles
- `Push Setup` in the Networking tab sends a **classroom setup bundle** to selected connected EDU peers.
- A setup bundle is for lesson-wide configuration, not student content.
- It can carry things like:
  - `teacher_mode`
  - default year level
  - Janet safety toggles
  - game or voice toggles
  - model hints and token limits
- It does **not** carry:
  - teacher PINs
  - secret answers
  - blocked-device lists
  - student identity data
  - the actual homework pack or revision pack content
- Received setup bundles land in their own inbox first and must be applied deliberately.
- This is useful when one teacher device is the lesson-prep machine and the rest of the room should mirror that ready state quickly.

## Memory and logs
- `Chatty's thoughts` is session-only. It shows recent message-pair context that the main chat is actively using and clears when the app closes.
- `Memory jogger` persists across sessions as a short local summary built from recent activity when the app closes.
- Sidebar entries may preview-truncate in the narrow panel, but they show fuller text on hover.
- Teacher-only Bookkeeper logs are available from File -> Models after teacher unlock. That tab is meant for local log search and support diagnosis, not for students.
- The in-app Bookkeeper tab is PIN-gated for convenience, but it is still just a local UI boundary; the underlying files remain local files on disk.

## Sandbox and scratchpad
- `Chatty_Sandbox/` lives under the active EDU data folder and is the only file area Chatty-EDU's sandbox tools are allowed to touch.
- Default files:
  - `Chatty_Sandbox/scratchpad/current.md`
  - `Chatty_Sandbox/scratchpad/task_ledger.md`
- Use the `Sandbox` tab when you want Chatty-EDU to stay grounded during a longer piece of work instead of relying only on the active chat context.
- The scratchpad is for free-form durable notes.
- The task ledger is for structured state:
  - current task
  - next step
  - open questions
  - files touched
  - working notes
- If Chatty-EDU decides it would help to read or write sandbox files, it now stages those requests in the chat bar for approval instead of silently running them.
- The approval ladder is:
  - `Seed ledger from current prompt`
  - `Defer actions`
  - `Preload + Continue`
  - `Approve`
  - `Approve + Continue`
  - `Reject`
- `Preload + Continue` is usually the smoothest option for longer multi-step tasks because it lets Chatty-EDU gather scratchpad, ledger, and relevant file context before continuing.
- The Sandbox tab editor can also promote the current buffer into:
  - the scratchpad
  - task-ledger notes
  - `Current task`
  - `Next step`
  - a compact summary back into the persistent `Memory jogger`
- This is still local-only and still approval-gated. It is meant to improve continuity, not to give the model unrestricted file access.

## CLI admin (quick)
`cargo run -- --mode cli`

- Enter teacher console: type `teacher`, then the PIN (default `0000`; type `forgot` to use the secret answer).
- Commands:
  - `generate_pack_md`
  - `transcribe_outgoing`
  - `convert_submissions_to_md`
  - `export_printables`
  - `export_rubrics`
  - `create_pack`
  - `create_pack_multi`
  - `export_pack_template`
  - `import_pack <path>`
  - `import_submissions`
  - `show_completed`
  - `mode class`
  - `mode free`
  - `games on`
  - `games off`
  - `allow_games_in_class`
  - `forbid_games_in_class`
  - `set_pin`
  - `set_secret`
  - `back`
- Outside the teacher console: `submit <assignment_id>`.

## Modules and hosted tools
- Chatty-EDU can now host drop-in standalone modules inside closable tabs.
- Modules can stay fully standalone and only add a thin EDU compatibility plug:
  - `manifest.json` for discovery
  - optional `visual_load.json` for hosted native or web UI
  - optional `bridge/status.json` and `bridge/log_sources.json` for module-reported status
- This means a tool can run normally outside Chatty-EDU and still be EDU-compatible when dropped into `modules/`.
- Builder docs live in:
  - `docs/MODULES.md`
  - `docs/MODULE_TEMPLATE_CHOOSER.md`
  - `docs/DEMO_MODULES.md`
  - `docs/MODULE_BRIDGE.md`
  - `docs/MODULE_VISUAL_LOAD.md`
  - `module_templates/`

Bundled EDU demo modules also live in `modules/demo_*` so teachers can inspect working examples before trying third-party modules.

## Networking (optional local peer mode)
- Chatty-EDU can optionally connect to other nearby Chatty-EDU instances on the same local Wi-Fi or LAN.
- Use the `Network` menu or open the `Networking` tab.
- Turn on `Make available for connectivity` on the device that should be visible.
- On another device, click `Refresh discovery`, then `Connect`.
- You can then send short local handoff notes between connected EDU instances.
- Use `Push Pack` for homework content, `Push Revision` for revision markdown, and `Push Setup` for lesson-wide EDU settings.
- Received homework packs, revision packs, and setup bundles all land in their own inboxes first so they can be previewed before apply.
- The transport now supports chunked text and binary/file-style payloads too, so future classroom modules are not stuck with tiny one-packet transfers.
- Click a device name to rename it locally if several nearby machines look too similar.
- Click the group chip (or `+ Group`) to tag a device by class, table, or role.
- Click `Trust` for devices you expect to use regularly in the room.
- Use `Export trusted list` if you want another teacher or support machine to inherit the same remembered classroom devices.
- Use `Import trusted list` on that other machine so you do not have to rebuild the pairing list by hand.
- Use `Export blocked list` if another teacher or support machine should inherit the same classroom deny rules.
- Use `Import blocked list` on that machine when you want the blocked-device policy to travel cleanly too.
- Use `Select Connected` when you want to act on the current active classroom set quickly.
- Use the `Find` box to search by name, device ID, address, or group label.
- Use `Copy ID` or `Copy info` when you need to confirm exactly which device is which.
- Device IDs now stay stable across restarts, so renamed classroom devices and blocked-device rules stay tied to the same machines.
- `Allow` approves a device for the current run.
- `Trust` remembers that device's stable ID for future classroom joins.
- `Block` denies the device until you deliberately unblock it.
- If you are hosting a classroom room and need to restart, look for `Resume saved session` in Networking when you come back.
- If another trusted teacher/support device should take over live, select it and use `Hand off host to selected peer`.
- If the host device vanishes mid-session, the remaining room devices can use `Take over as host` instead of rebuilding the classroom room from scratch.
- If the room was hosting a lesson/module session, use `Restore state to bridge` after recovery to put the last cached `shared_state.json` back where the hosted module expects it.
- Use `Re-share latest state` when rejoining student/support devices need the teacher's last good module session state again.
- `Replay cached assets` is the companion lane for module-linked lesson files/assets as that recovery path fills out.
- This is useful for nearby teacher machines, support machines, or local collaboration setups.
- It is off by default and is not required for normal student or classroom use.
- Chatty-EDU and Chatty-Cog do not accidentally mix here; they use different local networking identifiers.
- Custom names and group labels are only local list-management helpers on your machine.
- Current transfer ceiling is **8 MiB decoded payload size**, split into **64 KiB chunks** with delivery acknowledgement and retry.
- For a focused plain-language explanation, see `docs/NETWORKING.md`.

## Data layout (under `./data` or `--base-path`)
- `config/` settings, UI state, and Bookkeeper memory files
- `config/bookkeeper/` cold logs plus `memory_jogger.txt`
- `Chatty_Sandbox/` scratchpad, task ledger, and sandbox working files
- `homework/outgoing/` pack Markdown
- `homework/assigned/` pack JSON
- `homework/completed/` submissions
- `homework/marking/` marking Markdown
- `homework/printables/` student printables
- `homework/rubrics/` teacher rubrics
- `revision/notes/` saved revision notes and progress
- `revision/past_papers/` imported past papers and teacher revision materials
- `models/` GGUF files
- `modules/` built-in EDU modules plus drop-in hosted modules
- `themes/`, `runtime/`, `logs/`, `ide/`

## Safety and offline
- Offline-first; no internet or cloud calls in core flows.
- Content filter (Janet) is on by default.
- External process modules are disabled unless explicitly allowed.
- Optional local networking is LAN-only, off by default, and only used when a person enables it.
- The ECG indicator reads local system counters only. It does not send telemetry anywhere.
- The visible activity trace is intentionally part of the trust model: it gives schools and families a simple on-screen cue that the app is operating locally and not making hidden calls home as part of normal use.

## Troubleshooting
- Build errors (local-model): install CMake and a C or C++ toolchain, then rerun `cargo build`, or build without local-model support with `cargo build --no-default-features`.
- Model load errors (for example worker exits immediately or `GGML_ASSERT`): the GGUF is likely too new or uses an unsupported quantization for this build; try a different GGUF model or quant.
- No model selected on startup: drop a GGUF into `<base>/models/` and relaunch, or refresh via File -> Models. If only one model is present, the main AI uses it and Bookkeeper stays in keyword-only mode.
- Revision or chat helper says no valid model is selected: choose a GGUF via File -> Models and confirm the file still exists under `<base>/models/`.
- Support report: use File -> Copy Diagnostic report, or open the Diagnostics tab and copy it from there.
- Missing packs or submissions: click "Rescan packs + submissions" and confirm the correct `--base-path`.
- No EDU peers found in Networking: make sure both devices are on the same trusted local network, at least one device has `Make available for connectivity` turned on, and local firewall policy allows Chatty-EDU on the LAN.
- EDU peers are visible but still refuse to connect cleanly: check the `Compatibility note` line in Networking. If it mentions protocol/version mismatch, update the older Chatty-EDU copy so both sides are on a reasonably matching build generation.
- If one room device still uses an older local build from before the chunked-transfer upgrade, it will look incompatible until that older copy is rebuilt or updated.
- Several EDU peers are visible but hard to tell apart: use `Copy info` to compare IDs or addresses, then rename the devices you use often and add group labels if helpful.
- Missing worksheet, list, or attachment in student view: confirm it is included in `### Student Printable` or `attachments:` and not only mentioned in the instructions text.
- Odd symbols or broken table characters in chat: the app now normalizes most model output to plain readable text; if a model still emits malformed content, try a different prompt or GGUF.
- PIN issues: use Teacher menu or CLI `teacher` -> `forgot`, then set a new PIN.
