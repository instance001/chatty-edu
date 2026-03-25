# Resources

Sample and template artifacts that are safe to share publicly. All IDs and names are placeholders; no student or private data is included.

- `homework_pack_example.json`: minimal homework pack example for quick schema reference (self-contained).
- `homework_pack_sample_bundle.json`: multi-assignment bundle; Reading Trends is self-contained, Soil Moisture uses the attached field notes.
- `homework_pack_template.md`: Markdown-first homework pack template (transcribe via Teacher Dashboard into `homework/assigned/`; includes optional Student Printable and Rubric sections).
- `submission_example.json`: example submission payload showing pre-mark structure.
- `attachments/soil_moisture_gradient.txt`: dummy field-notes attachment referenced by the Soil Moisture assignment.
- Model guidance: see `resources/models/qwen/README.md` for supported Qwen variants and licensing notes (no weights included; bring your own GGUF).

Usage tips:
- Copy a pack from this folder into your runtime data directory (for example `homework/assigned/`) before running the app, or import it via the GUI or CLI.
- If a pack references attachments, copy the `attachments/` folder alongside it.
- For Markdown-first packs, copy `homework_pack_template.md` into `homework/outgoing/`, edit it, then use Teacher Dashboard -> "Transcribe outgoing (.md -> .json)".
- In Markdown pack files, `year_level` is the canonical key, but Chatty-EDU also accepts `year`, `year level`, `grade`, `grade level`, and `year group` when importing or transcribing.
- If an assignment mentions a worksheet, list, table, chart, or handout, include that material in `### Student Printable` or `attachments:` so students can see it in-app.
- The sample and template files are useful for testing the `.md -> .json -> .md` homework pipeline, the student material preview flow, and the completed-submission inputs that later feed Revision.
- Keep real submissions and packs out of version control; use these samples or your own sanitized templates instead.
