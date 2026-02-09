# Glossary (Repo Excerpt)

For the full glossary, see: https://github.com/instance001/Whatisthisgithub/blob/main/GLOSSARY.md

This file contains only the glossary entries for this repository. Mapping tag legends and global notes live in the full glossary.

## chatty-edu
| Term | Alternate term(s) | Alt map | External map | Relation to existing terminology | What it is | What it is not | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Chatty-EDU | chatty-edu | = | ~ | Matches an offline/local-first educational assistant application | Rust/egui desktop + CLI app for schools; runs fully offline with user-supplied GGUF models | Not cloud-connected; not bundled with model weights; not telemetry-enabled | chatty-edu/README.md |
| Teacher lock (PIN) | teacher PIN, teacher menu lock | ~ | ~ | Analogous to admin PIN gating | PIN-gated teacher dashboard/console (default 0000) with secret question/answer recovery; meant to be changed on first use | Not a security-grade auth system; not student-facing | chatty-edu/README.md; chatty-edu/teacher_user_manual.md |
| Homework pack | homework_pack_*.json | = | ~ | Equivalent to assignment manifest | JSON schema v1 describing assignments (id/title/subject/year/due, instructions_md, allow_games, allow_ai_premark, max_score, attachments) | Not a submission; not coupled to any specific model | chatty-edu/README.md |
| Submission file | submission_*.json | = | ~ | Comparable to student submission artifact | JSON schema v1 capturing answers, attachments, ai_premark, hash-chained event log with final_hash for tamper-evidence | Not the homework pack; not encrypted telemetry | chatty-edu/README.md; chatty-edu/teacher_user_manual.md |
| Module manifest | modules/<id>/module.json | = | ~ | Similar to plugin/feature descriptor | Declares module id/title/roles/version and entry type (builtin_panel/markdown/static_html; external_process disabled by default) | Not executable code by default; external processes gated/disabled | chatty-edu/README.md |
| Homework & Revision tutor | hints-only tutor, LLM homework helper | ~ | ~ | Partial analogue to guided tutoring with safety rails | Module that provides hints and LLM-assisted guidance tied to the selected assignment; hints-only mode configurable by teacher | Not a full-answer generator; not globally scoped beyond selected assignment | chatty-edu/README.md |
| Janet content filter | Janet filter | ~ | ~ | Similar to offline safety filter | Default offline content filter applied across chat and tutor interactions | Not cloud moderation; not disabled by default | chatty-edu/README.md |
| GGUF local model slot | local model | ~ | ~ | Standard local GGUF model usage | User-provided GGUF placed in data/models/ and selected via File → Models; model-agnostic | Not bundled weights; not reliant on Ollama or internet | chatty-edu/README.md |
| Hash-chained event log | submission event log, final_hash | ~ | ~ | Comparable to tamper-evident audit log | Sequence of submission events (start/answer/hint/retry/finalize) linked by hashes, ending with final_hash in submission JSON | Not a cryptographic signature of content authenticity; local-only evidence | chatty-edu/teacher_user_manual.md |
| Modes: class/free | class mode, free mode | ~ | ~ | Operational modes akin to classroom vs open use | Teacher-configurable modes affecting games and controls via CLI/GUI | Not tied to network profiles; not enforcing identity management | chatty-edu/teacher_user_manual.md |
