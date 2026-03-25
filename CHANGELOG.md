# Changelog

## v0.5
- Added the EDU tri-helix memory layer to Chat: `Chatty's thoughts` is now a session-only left sidebar, `Memory jogger` is a persistent right sidebar, and teacher-only Bookkeeper cold-log search is available behind the Teacher PIN.
- Wired the memory tiers into the main AI flow: current-session message pairs now feed the main chat prompt directly, while the Bookkeeper rolls cold-log entries into a structured persistent memory jogger summary on shutdown.
- Added boot-time model auto-assignment: largest detected GGUF becomes the main AI role, smallest becomes the Bookkeeper role when multiple models exist, and single-model installs keep Bookkeeper in keyword-only summary mode.
- Improved chat and memory rendering safety: prompt-template markers are stripped, odd Unicode or mojibake is normalized into plain readable text, and unsupported punctuation or table glyphs degrade more gracefully instead of broken boxes.
- Chat and memory sidebars now use matched fixed widths, memory preview entries truncate only in-panel with fuller hover text, and both `Chatty's thoughts` and `Memory jogger` support hover expansion.
- Added a small ECG activity widget across the GUI that samples Windows hardware counters on a roughly 1.5 second cadence and shows recent GPU-first system activity with CPU fallback, framed as a transparency and trust feature for the zero-calls-home design.
- Split live Homework and Revision into separate workflows: Home now owns active assignments and submissions, while Revision works from completed homework plus past papers and stores notes under `revision/`.
- Home tab now includes a compact mirror of the live chat so students can keep working without switching tabs to see the conversation.
- Main Chat is now homework-aware: it uses the selected assignment context when relevant and shows the same homework-context banner used by the dedicated chat view.
- Added an app-level homework question intercept: active assignment questions are extracted on pack load, fuzzy-matched before send, and forced into a hints-only override even if the student rephrases the homework as a general question.
- Hardened homework hint behavior so answer-like model outputs are clamped back to a safer Socratic hint response.
- Home now renders worksheet or handout content plus text attachment previews when available, and warns when an assignment appears to refer to a missing list or resource.
- Improved pack hygiene by normalizing loaded packs, merging duplicate assignments, and cleaning malformed fenced Markdown from AI-generated printable or rubric sections.
- Pack generation and transcription now accept year or grade metadata aliases such as `year`, `year level`, `grade`, `grade level`, and `year group`, while keeping `year_level` as the canonical key.
- Revision now hides teacher-facing scores and diagnostic labels from the student view, uses a qualitative confidence check-in, handles short social cues like `thanks`, and returns friendlier model-not-found messages without raw filesystem paths.
- Fixed the teacher Homework Dashboard `egui` ID collision warning by assigning stable IDs to the nested scroll areas and row widgets in the student-selection and submissions sections.
- Documentation refreshed to reflect the ECG widget, tri-helix memory flow, auto model selection, separated Revision flow, homework-aware chat, hint interception, student material previews, and current teacher or student flows.

## v0.5
- Markdown-first homework workflow: author packs in `homework/outgoing/*.md` and transcribe to `homework/assigned/*.json`.
- Marking export: convert completed `submission_*.json` into `homework/marking/marking_*.md`.
- Paper workflow support: optional `### Student Printable` and `### Rubric` or `### Marking Guide` sections in pack Markdown, plus one-click exports to `homework/printables/` and `homework/rubrics/`.
- Teacher Dashboard AI helper for drafting pack Markdown, plus one-click transcribe and export buttons.
- CLI parity for the same flows (`generate_pack_md`, `transcribe_outgoing`, `convert_submissions_to_md`).
- Build improvements: local model support is optional with `--no-default-features`; manifest filename fixed for case-sensitive systems.
- Stability: local model runs in an internal worker subprocess so incompatible GGUF models fail gracefully instead of hard-crashing the app.
- Diagnostics: Health Check tab is scrollable and now includes GGUF metadata (architecture, name, file type, context length, tensor types) to help debug model compatibility.
- Docs: added `it_deployment_guide.md` and `security_privacy_statement.md`.

## v0.4
- Public-ready release with cleaned repository structure.
- Offline, bring-your-own-model design clarified (no model weights included).
- Resources folder added with model attribution and licenses.
- Hash-chained homework submissions for tamper-evidence.
- Chat and homework UX stabilized (scrolling and theme contrast fixes).
- Documentation aligned for teachers, students, and reviewers.

## v0.3
- GUI shell (`eframe` or `egui`) with menus, tabs, themes, and built-in Homework Dashboard.
- Homework packs: import or export, assignment and subject filters, class or student metrics.
- Submissions: answer entry, attachments, simple AI pre-mark with score and feedback.
- Module system with built-in dashboard module and drop-in manifest support.
- Portable data layout (`./data` by default) plus `--base-path` override; offline by default.

## v0.2
- Config loader, runtime helpers, basic state machine (CLI-focused).

## v0.1
- Initial CLI spine and folder scaffolding.
