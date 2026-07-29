# chatty-edu Student Manual (v0.5)

Audience: students with zero setup experience. Everything runs offline by default; no accounts. Some schools may also use optional local Wi-Fi or LAN connectivity between nearby chatty-edu machines.

That optional peer mode is only for nearby chatty-edu machines. It does not connect to chatty-cog.

## Quick start (prebuilt)
1. Open `chatty-edu.exe` or run it from a terminal in the folder provided to you.
2. Models: if a GGUF is already present in the active `models/` folder, Chatty-EDU will try to auto-select it at startup. In a prebuilt folder, that usually means `data/models/` beside `chatty-edu.exe`. If your teacher gives you a GGUF, put it there and choose it via File -> Models if needed. Model guidance lives in `resources/models/` (for example `resources/models/qwen/README.md`).
3. Import homework: Home tab -> "Import pack file" to load `homework_pack_*.json`, or copy it into `homework/assigned/` under the active base path.
4. Pick your assignment on Home and read the instructions.
5. Under "Submit work", type your answers. Add attachments if your teacher asked for them.
6. Click "Export submission file" to save `submission_<assignment_id>_<your_id>.json` into `homework/completed/` under the active base path. Upload that JSON and any attachments using your normal hand-in method.

## Quick start (build yourself)
1. Install Rust (`https://rustup.rs`). Local model builds also require CMake and a C or C++ toolchain.
2. In the project folder, run `cargo run -- --mode gui` or `cargo run -- --mode cli`.
3. To run without local-model support, use `cargo run --no-default-features -- --mode gui`.
4. Follow the same steps as the prebuilt quick start.

## Homework
- Homework packs live in `homework/assigned/` under the active base path. If you do not see yours, click "Rescan packs + submissions" on Home.
- Home shows your selected assignment, any worksheet or handout text included with it, and previews of text attachments when possible.
- The shared chat bar on Home and the main Chat tab both become homework-aware when an assignment is active. If you ask the assignment directly there, or rephrase one of the active homework questions, Chatty may still give you a hint instead of the final answer.
- Home includes a small live chat mirror at the bottom so you can keep working without leaving the page to see the latest reply.
- Chat tab is still the main full conversation view for general learning questions.
- On the Chat tab, `Chatty's thoughts` appears on the left and shows what Chatty can still remember in the current session. It clears when the app closes.
- On the Chat tab, `Memory jogger` appears on the right and shows short reminders from recent sessions. It stays across restarts.
- Sidebar entries may shorten inside the narrow panel, but you can hover them to read more.

## Revision
- Revision is separate from live homework. It uses work you have already submitted, plus any past papers your teacher imported.
- Revision can save your own notes or progress under `revision/notes/`.
- The Revision helper can explain more openly than live homework help because the work is already submitted, but it is still meant to teach and guide rather than just dump answers.
- Teacher-side scores or diagnostic labels are not shown in the student Revision view.

## Staying in one workflow

You do not always have to bounce between lots of separate windows to use Chatty-EDU well.

A common pattern is:

1. Read the assignment or revision material on Home.
2. Ask for a hint or explanation in Chat.
3. Keep your own notes or working draft in the app's local workflow areas.
4. Return to the assignment or revision screen with that context still fresh.

If your school uses hosted modules, the same idea can extend a bit further:

- the main AI can help explain the next step
- the hosted module can keep a more specialized tool visible in a tab
- the app can carry forward recent context instead of making you restart from scratch each time

In plain language:

- ask
- work
- check
- improve
- continue

That is useful for longer tasks because Chatty-EDU is designed to help you keep the workflow moving inside one local app rather than forcing you to juggle lots of separate windows.

## File locations

Chatty-EDU creates these folders on first run. In a prebuilt folder, they usually live under `data/` beside `chatty-edu.exe`. If the app was started with `--base-path <path>`, they live under that path instead.
- `homework/assigned/` - homework packs (`homework_pack_*.json`)
- `homework/completed/` - your exported submissions (`submission_*.json`)
- `revision/notes/` - your saved revision notes or progress
- `revision/past_papers/` - past papers your teacher imported for revision
- `models/` - local GGUF models; pick via File -> Models
- `config/bookkeeper/` - local memory jogger and teacher-side log files
- `config/`, `themes/`, `modules/` - app settings and themes (usually leave these alone)

## Tips
- If you change computers, keep the active data folder with you. Run with `--base-path <USB path>` to force data onto the USB.
- If the tutor says it cannot give the answer, ask for steps, a starting point, or the rule you need.
- If your homework refers to a worksheet, list, or table and you cannot see it, tell your teacher the pack may be missing the extra material.
- If your school uses the Networking tab, that is still local school-network sharing between nearby Chatty-EDU devices, not internet chat.
- It is also only for Chatty-EDU devices. A nearby ChattyCog machine is not a compatible peer target.
- If your school uses network-delivered packs or setup bundles, they should appear in inboxes first rather than silently changing your device.
- Teacher-only settings such as filters, games, and hints-only behavior are locked behind the Teacher PIN.
- The ECG widget in the top-right corner is a small machine-activity indicator for the local computer. It is there partly for transparency, so you, your teacher, or your family can see when Chatty-EDU is actively doing local work.

## Staying offline and safe
- Chatty-EDU works without internet; no cloud calls in normal use.
- Some schools may choose to use optional local networking between nearby Chatty-EDU devices on the same Wi-Fi or LAN, but that is still local and not the public internet.
- Content filter (Janet) is on by default to block swears and mature topics.
- The app stores homework packs, submissions, and chat context locally on the machine you are using.
- Homework packs, revision packs, and classroom setup bundles can be shared locally between nearby EDU devices, but they still stay inside the local school environment and are not internet syncing.
- `Chatty's thoughts` is session-only and clears on close. `Memory jogger` is a local cross-session reminder saved on the same machine.
- The ECG widget is part of that trust story: it is a visible local activity cue, not a hidden tracker, and it helps show that Chatty-EDU is doing work on your computer rather than calling home.
