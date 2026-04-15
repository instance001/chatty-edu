# Teacher Notebook (Demo)

`Teacher Notebook (Demo)` is a native Python/Tkinter example for short observation notes, support ideas, and follow-up checkpoints.

## What it shows

- a hosted native-window module
- standalone state in `state.json`
- optional Chatty-EDU bridge handoff
- optional module-owned log tails through `bridge/log_sources.json`

## Standalone use

Run `py -3 src/main.py` from this folder.

## Hosted use

Drop the folder into `chatty-edu/modules/` and open it from the Modules menu. Chatty-EDU can dock the real notebook window into a tab and read the bridge handoff when needed.
