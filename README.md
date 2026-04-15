# Chatty-EDU v0.5.0

Offline, local-first learning assistant for schools. No cloud, no accounts, no tracking. Ships as a single Rust binary with an egui desktop shell (Windows first) plus a CLI mode. Licensed under AGPLv3.

Chatty-EDU never connects to the internet and does not require external services to function. It can also optionally connect to other nearby Chatty-EDU instances over local Wi-Fi or LAN when a user deliberately enables local networking.

Designed for schools and boards:
- Runs entirely on school hardware; bring your own offline model (GGUF).
- Teacher PIN is meant to be changed on first login; keep the teacher menu locked in student-facing setups.
- Default PINs and secrets exist only for first-run convenience and are intended to be changed immediately.
- Districts can drop in their preferred models; documentation includes guidance for recommended small models, which can be replaced as needed.
- Works without internet; external processes are disabled unless explicitly allowed.

Design intent and boundaries: see `design_intent.md`. Public-safe sample packs and submission templates live in `resources/`.

Docs:
- `student_user_manual.md`
- `teacher_user_manual.md`
- `it_deployment_guide.md`
- `security_privacy_statement.md`
- `design_intent.md`
- `GLOSSARY.md`
- `CHANGELOG.md`
- `docs/MODULES.md`
- `docs/MODULE_TEMPLATE_CHOOSER.md`
- `docs/DEMO_MODULES.md`
- `docs/NETWORKING.md`
- `docs/MODULE_BUILDER_CHECKLIST.md`
- `docs/MODULE_PACKAGING_GUIDE.md`
- `module_templates/`

## Current highlights
- Markdown-first homework packs: author `*.md` in `homework/outgoing/` and transcribe to JSON packs in `homework/assigned/`.
- Marking exports: convert `submission_*.json` into `homework/marking/marking_*.md` for review and marking.
- Paper workflow: optional `### Student Printable` and `### Rubric` or `### Marking Guide` sections in pack Markdown, plus exports to `homework/printables/` and `homework/rubrics/`.
- Teacher Dashboard AI helper: optionally draft a pack `.md` using the local model, then save and transcribe it.
- Tri-helix memory surfaces: Chat includes `Chatty's thoughts` on the left for current-session context, `Memory jogger` on the right for persistent recent-session summaries, and a teacher-only Bookkeeper log search view behind the Teacher PIN.
- Chatty sandbox: `Chatty_Sandbox/` now gives Chatty-EDU a real local scratchpad, task ledger, and approval-gated file tool lane for longer multi-step work without pushing raw file access outside the EDU data folder.
- Homework-aware chat guardrails: both the Homework hint helper and main Chat intercept active assignment questions and steer the model toward hints instead of answers.
- Student-facing homework context: Home shows the selected assignment, worksheet or handout content, text attachment previews when available, the submission flow, and a chat mirror.
- Separate Revision workflow: Revision pulls from completed homework, keeps revision notes or progress under `revision/notes/`, supports imported past papers under `revision/past_papers/`, and uses a looser revision helper than live homework.
- Standalone module hosting: drop-in modules can now advertise their own native desktop window or browser-style dashboard, and Chatty-EDU can host that real standalone UI in a tab without taking over the module's runtime.
- Bundled EDU demo modules: lesson planning, revision sprinting, and a native teacher notebook now ship as living examples for educators and builders.
- Optional local networking: nearby Chatty-EDU instances can discover and connect over local Wi-Fi or LAN, share lightweight presence, send short handoff notes, and move homework packs, revision packs, and classroom setup bundles without using cloud services.
- Home tab chat mirror: students can keep working on Home while still seeing the latest chat exchange at the bottom of the page.
- ECG window: a small top-right activity trace acts as a transparency and trust feature, showing visible local activity using Windows hardware counters with GPU-first fallback to CPU.
- Pack parsing is forgiving around year or grade terminology: `year_level` is canonical, but Markdown import or transcribe also accepts `year`, `year level`, `grade`, `grade level`, and `year group`.
- Automatic model role selection: on boot, Chatty-EDU scans `data/models/`, gives the largest valid GGUF to the main chat role, gives the smallest to the Bookkeeper role when 2+ models are present, and falls back to a friendly setup message when no model is available.
- Plain-text-safe rendering: model output is normalized before display so unsupported Unicode, prompt-template markers, and odd table characters degrade into readable text instead of broken glyphs.
- CLI parity: teacher console can `generate_pack_md`, `transcribe_outgoing`, and `convert_submissions_to_md`.
- Easier builds: local-model support is optional with `cargo build --no-default-features`.

## Project layout (auto-created under `./data` or `--base-path`)
- `config/` - settings, UI state, and Bookkeeper memory files
- `config/bookkeeper/` - `cold_log.jsonl` plus persistent `memory_jogger.txt`
- `homework/outgoing/` - teacher-authored packs in Markdown (`*.md`) to be transcribed into `homework/assigned/`
- `homework/assigned/` - homework packs (`homework_pack_*.json`)
- `homework/completed/` - submissions (`submission_*.json`)
- `homework/marking/` - marking sheets exported as Markdown from student submissions (`marking_*.md`)
- `homework/printables/` - student printables exported as Markdown (`student_*.md`)
- `homework/rubrics/` - teacher rubrics or marking guides exported as Markdown (`rubric_*.md`)
- `modules/` - built-in EDU modules plus drop-in modules (`manifest.json` preferred, `module.json` legacy-supported, optional `visual_load.json` and bridge files)
- `modules/demo_*` - bundled EDU-flavoured hosted demo modules
- `themes/` - active theme plus presets
- `models/` - drop offline GGUF model files here; select via File -> Models
- `revision/notes/` - saved revision notes or progress
- `revision/past_papers/` - imported past papers and teacher revision materials
- `network_inbox/` - received networking items waiting in inboxes before apply
- `network_inbox/homework_packs/` - received homework packs
- `network_inbox/revision_packs/` - received revision packs
- `network_inbox/workflow_bundles/` - received classroom setup bundles
- `Chatty_Sandbox/` - approval-gated scratchpad, task ledger, and working files for longer local tasks
- `runtime/`, `logs/`, `ide/` - reserved for expansion

## Prereqs
- Rust toolchain (`https://rustup.rs`).
- Local model builds (default) compile llama.cpp via CMake, so you also need CMake and a C or C++ toolchain.
- To build without the local model backend: `cargo build --no-default-features`.

## Models (bundled plus swap-in)
- Model binaries are not included in the repo; drop an approved GGUF into `data/models/` (or your chosen `--base-path`) and select it via File -> Models.
- Model-agnostic: drop in your preferred GGUF models and select them via File -> Models; districts are expected to use their approved models.
- At startup, Chatty-EDU auto-scans `data/models/`: the largest valid GGUF becomes the main AI, the smallest becomes the Bookkeeper role when 2+ models exist, and a single-model install keeps Bookkeeper in keyword-only mode.
- If no GGUF is present, the app shows a friendly "drop a GGUF into `data/models/` to get started" message instead of a raw path error.
- The local model runs in an internal worker process so incompatible GGUFs fail with an error instead of hard-crashing the app.
- Very large models may still be slow or exceed RAM on school devices; prefer smaller GGUFs.
- Model guidance and attribution: see `resources/models/` (for example `resources/models/qwen/README.md`) for supported third-party variants and licensing notes; no weights are shipped.

## Build and run
```bash
cargo build
cargo build --no-default-features
cargo build --release

# GUI (default)
cargo run -- --mode gui

# CLI
cargo run -- --mode cli

# Custom data location (for example a USB drive)
cargo run -- --mode gui --base-path D:\ChattyData
```

## GUI overview
- Menus: File / View / Modules / Tools / Network / Teacher / Settings / Help.
- Tabs: Home, Chat, Settings, Diagnostics, Homework Dashboard (module), and Revision (module).
- Sandbox: the `Sandbox` tab hosts `Chatty_Sandbox/`, including the default scratchpad at `scratchpad/current.md`, the structured task ledger at `scratchpad/task_ledger.md`, a recursive file list, and a built-in editor.
- Networking: open the `Networking` tab from `View` or `Network` to make this device visible on the local LAN, discover nearby EDU peers, connect, disconnect, send handoff notes, push homework packs or revision packs, send classroom setup bundles, and locally rename/group devices so larger classroom lists stay manageable.
- Modules: drop-in modules can open as closable tabs, host their own real native or web UI when they advertise `visual_load.json`, and optionally report status back through the portable EDU bridge.
- Demos: bundled hosted demo modules live under `modules/demo_*` so builders can inspect working portable examples.
- Models: File -> Models to pick a GGUF from `data/models/`, inspect the current auto-assigned main or Bookkeeper roles, refresh after you drop one in, or open the teacher-only Bookkeeper logs tab once unlocked.
- Teacher lock: Teacher menu -> unlock with PIN (default PIN `0000`, intended to be changed on first teacher unlock) or secret answer (default answer `Math`, also intended to be changed on first teacher unlock). Teacher Dashboard is hidden until unlocked.
- Homework packs: author packs in Markdown under `data/homework/outgoing/` and transcribe to JSON via the Teacher Dashboard, or import JSON packs via Home or the Teacher menu.
- Sharing: homework packs are simple files. Share a pack `.md` or `.json` with other teachers or schools, including any referenced attachments.
- Setup bundles: use `Push Setup` in Networking when you want to mirror lesson-wide EDU settings to selected nearby devices without also pushing the actual homework or revision content.
- Home tab: selected assignment preview, worksheet or handout rendering, text attachment previews when possible, submission area, and a compact mirror of the live chat.
- Chat: the bottom input bar is shared with the main chat flow; when homework context is active, Chat can use the selected assignment and still answer unrelated questions normally. The Chat tab also shows `Chatty's thoughts` on the left and `Memory jogger` on the right; sidebar entries preview-truncate in place and show fuller text on hover.
- Sandbox approvals: when Chatty-EDU wants to read, write, append, preload, or update the task ledger inside `Chatty_Sandbox/`, it now stages those actions for approval instead of silently running them. The chat bar supports `Seed ledger from current prompt`, `Defer actions`, `Preload + Continue`, `Approve`, and `Approve + Continue`.
- Scratchpad flow: use the Sandbox tab toolbar to promote working text into the scratchpad, turn it into task-ledger notes, set `Current task` or `Next step`, or append a compact summary back into the persistent `Memory jogger`.
- Homework guardrails: Home and Chat both intercept active assignment questions and steer them back toward hints instead of full answers.
- Revision module: separate from live homework; it uses completed submissions plus past papers, stores notes under `revision/notes/`, and gives a more open revision helper while hiding teacher-side scores or diagnostic labels from student view.
- Submissions: type answers, add attachments, export submission JSON with a hash-chained event log (`start`, `answer`, `hint`, `retry`, `finalize`) and `final_hash` for tamper-evidence.
- Marking: Teacher Dashboard can convert completed submission JSON files into Markdown marking sheets under `data/homework/marking/`.
- Printables: Teacher Dashboard can export student printables to `data/homework/printables/`.
- Rubrics: Teacher Dashboard can export teacher rubrics to `data/homework/rubrics/`.
- Metrics: class or subject averages, per-student bars, multi-student selection, submissions summary, and separate revision tools in the Homework Dashboard.
- ECG window: a small indicator in the tab chrome shows recent system activity and refreshes on a lightweight hardware polling cycle. Its purpose is transparency as much as diagnostics, so schools, students, and parents can see when Chatty-EDU is actively doing local work.
- Themes: switch via View; presets include `classic_light`, `chalkboard_dark`, and `high_contrast`.
- Homework materials: if a task refers to a worksheet, list, or attachment, include it in `### Student Printable` or `attachments:` so students can actually see it in-app.

## CLI quick commands
- `submit <assignment_id>` - prompt for answers or attachments; writes submission JSON to `homework/completed/`.
- `teacher` - enter teacher console (default PIN `0000`; intended to be changed on first teacher unlock); type `forgot` to answer the secret question (default answer `Math`; intended to be changed on first teacher unlock). Inside teacher console:
  - Packs: `generate_pack_md` (AI draft to `homework/outgoing/`), `transcribe_outgoing` (outgoing `.md` -> assigned `.json`), `import_pack <path>` (import `.md` or `.json`)
  - Marking: `convert_submissions_to_md` (submission `.json` -> `homework/marking/*.md`)
  - Paper exports: `export_printables` (pack -> `homework/printables/*.md`), `export_rubrics` (pack -> `homework/rubrics/*.md`)
  - Builders: `create_pack`, `create_pack_multi`, `export_pack_template`
  - Review: `import_submissions`, `show_completed`
  - Mode controls: `mode class`, `mode free`
  - Game controls: `games on`, `games off`, `allow_games_in_class`, `forbid_games_in_class`
  - PIN: `set_pin`
  - Secret: `set_secret`

## Modules (summary)
Chatty-EDU now supports two module styles:

- preferred portable modules using `modules/<id>/manifest.json`
- legacy EDU modules using `modules/<id>/module.json`

Portable modules can also advertise:

- `visual_load.json` to host the real standalone UI inside a tab
- `bridge/status.json` and `bridge/log_sources.json` for optional EDU-side status and log context

Legacy `module.json` modules still work for:

- `builtin_panel`
- `markdown`
- `static_html`
- `external_process` (still gated / not the recommended new-module path)

For the full builder path, use:

- `docs/MODULES.md`
- `docs/MODULE_VISUAL_LOAD.md`
- `docs/MODULE_BRIDGE.md`
- `docs/DEMO_MODULES.md`
- `module_templates/`

## Homework pack schema (v1)
```json
{
  "version": "1.0",
  "school_id": "school-123",
  "class_id": "yr7-math-a",
  "created_at": "2026-01-01T00:00:00Z",
  "assignments": [
    {
      "id": "hw-001",
      "title": "Fractions",
      "subject": "Math",
      "year_level": "7",
      "due_at": "2026-01-05T09:00:00Z",
      "instructions_md": "Solve the attached problems...",
      "student_printable_md": "Optional: a paper-friendly student handout (defaults to instructions_md if omitted)",
      "teacher_rubric_md": "Optional: teacher rubric or marking guide (included in marking exports)",
      "allow_games": false,
      "allow_ai_premark": true,
      "max_score": 100,
      "attachments": []
    }
  ]
}
```

For Markdown-first packs, `year_level` is the canonical metadata key, but import or transcribe also accepts `year`, `year level`, `grade`, `grade level`, and `year group`.

## Submission schema (v1)
```json
{
  "version": "1.0",
  "school_id": "school-123",
  "class_id": "yr7-math-a",
  "assignment_id": "hw-001",
  "assignment_title": "Fractions",
  "assignment_subject": "Math",
  "assignment_year_level": "7",
  "assignment_instructions_md": "Solve the attached problems...",
  "student_id": "s12345",
  "student_name": "Sample Student",
  "submitted_at": "2026-01-02T15:30:00Z",
  "answers_text": "My work...",
  "answers": [],
  "ai_premark": { "score": 78, "feedback": "Check step 3." },
  "attachments": ["path/to/work.pdf"],
  "events": [
    {
      "t": 1764000000,
      "type": "start",
      "qid": null,
      "payload": null,
      "prev_hash": "",
      "hash": "..."
    }
  ],
  "final_hash": "...",
  "summary": "Optional short summary"
}
```

## Safety and offline stance
- Offline by default; no internet or cloud calls in core flows.
- External process modules are disabled unless explicitly allowed.
- Content filter (Janet) is enabled by default and operates entirely offline.
- Optional local networking is LAN-only, off by default, and only activates when a user enables local peer connectivity.
- Homework packs, submissions, and AI pre-mark outputs are stored locally as readable JSON files.
- Bookkeeper memory stays local under `config/bookkeeper/`; `Chatty's thoughts` is session-only, while `Memory jogger` persists across sessions as local text.
- The ECG window reads local Windows performance counters only; it is a local transparency feature and UI health indicator, not telemetry.
- It exists partly to make activity visible in the room, reinforcing the zero-calls-home design with an always-on local signal that Chatty-EDU is doing work on-device rather than silently sending data elsewhere.
- There is no telemetry, analytics, logging to third parties, or remote kill-switch.
