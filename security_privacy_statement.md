# Chatty-EDU Security & Privacy Statement (v0.5)

This document describes the security and privacy posture of Chatty-EDU as shipped in this repository. It is intended for school IT, teachers, parents, and reviewers.

## Summary (plain language)
- Chatty-EDU is offline-first and does not require accounts or cloud services.
- In normal operation, it stores data as local files under a configurable base directory (default: `./data` next to the executable).
- Prompts and responses for the local model are processed on the same machine. No text is sent to any server by Chatty-EDU.
- The ECG window is a visible transparency feature: it shows local machine activity so schools, students, and parents have an on-screen cue that the app is actively working on-device.

## What data is stored
All data is stored on disk under the base path:
- `config/settings.json`
  - teacher PIN and secret question or answer (local gating only)
  - model path and token limits
  - basic student profile fields (name, id, class) used for submissions
- `config/bookkeeper/cold_log.jsonl`
  - append-only local session log used by the teacher-only Bookkeeper search view
- `config/bookkeeper/memory_jogger.txt`
  - persistent local memory-jogger summary used to carry recent context across sessions
- Homework packs and submissions
  - `homework/assigned/homework_pack_*.json` (teacher-authored pack content)
  - `homework/completed/submission_*.json` (student submissions, attachments list, event log, and a hash-chain for tamper-evidence)
- Revision data
  - `revision/notes/revision_note_*.json` (student revision notes and progress)
  - `revision/past_papers/*` (teacher-imported revision materials or past papers)
- Exports
  - `homework/marking/marking_*.md` (marking sheets)
  - `homework/printables/student_*.md` (student printables)
  - `homework/rubrics/rubric_*.md` (teacher rubrics or marking guides)

Chatty-EDU does not include telemetry, analytics, or a remote logging service.
Teacher-side AI premark data may exist inside local submission files, but the student Revision view hides raw score and diagnostic labels.
`Chatty's thoughts` is not a separate saved file; it is session-only context that clears when the app closes.

## Transparency signal: ECG window
Chatty-EDU includes a small ECG-style activity indicator in the GUI.
- It reads local Windows performance counters only.
- It is designed as a trust and transparency feature, not as analytics.
- It helps make "the app is busy locally right now" visible to the person using the machine and anyone supervising or reviewing the setup.
- It supports the product's zero-calls-home posture by making activity observable on screen instead of asking users to trust a hidden background process.

Important limitation:
- The ECG window is not a packet sniffer or formal network-audit tool. It is a visible local activity cue that reinforces the offline design; schools that require formal network verification should still use their standard firewall, DNS, and endpoint monitoring controls.

## Network behavior
Chatty-EDU is designed to operate without internet access.
- The core application does not need network access to function.
- Chatty-EDU does not ship with any cloud endpoints and does not transmit homework or submission content to third parties as part of normal operation.
- The ECG window does not contact any service; it only reads local counters and gives users a visible signal that work is happening on-device.

Important exceptions:
- If a user clicks external links, the OS may launch a browser that can access the network.
- If you modify the app to add online features, or enable or run external tools outside Chatty-EDU, those tools may use the network.

## Authentication and access control (important limitation)
Teacher tools are gated by a local PIN or secret stored in `config/settings.json`.
- This is intended to reduce accidental access in a classroom UI.
- It is not a hardened security boundary against a user who can access the filesystem.
- The teacher-only Bookkeeper tab follows the same model: it is hidden behind the Teacher PIN in-app, but the underlying files are still local filesystem artifacts.

Recommended controls:
- Use OS-level accounts and permissions (separate teacher or admin accounts where needed).
- Restrict read or write access to the base directory.
- Use disk encryption and standard endpoint protections.

## Tamper-evidence (submissions)
Submissions include a hash-chained event log and a `final_hash` field. This can help detect casual modification of submissions after the fact.

Limitations:
- This is not a cryptographic signature tied to a trusted identity.
- A user with full filesystem access could still replace files; the hash-chain is best treated as a lightweight integrity signal.

## Local model safety and privacy
- The local model runs on-device. Prompts and outputs remain on the same machine.
- Model weights are not shipped; schools bring their own approved GGUF.
- The built-in Janet filter can block some unsafe topics or words, but no filter is perfect and the model can still be wrong.
- Revision uses completed submissions as source material and may use local teacher-side signals in the background, but those raw score or diagnostic labels are not shown in the student Revision UI.

Operational guidance:
- Treat AI output as assistive, not authoritative.
- Use smaller approved models for reliability on low-end machines.

## Data retention and deletion
Chatty-EDU does not automatically delete data.
- To remove data, delete the base directory or the specific subfolders you no longer need.
- For a clean reset, remove `<base>/config/settings.json` and the relevant `homework/` or `revision/` subfolders.

## Security responsibilities (school side)
Because Chatty-EDU is local-first, security depends heavily on device controls:
- Physical access to devices
- Windows user accounts and permissions
- Antivirus, EDR, and patching
- Backups and recovery processes

## Scope and disclaimers
- This statement reflects the behavior of the open-source project as configured here; forks or custom builds may differ.
- Chatty-EDU is not represented as a certified compliance product. Schools should evaluate fit for their own policy and regulatory context.
