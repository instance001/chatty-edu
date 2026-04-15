# Template Module Handshake

- **module_id**: `template_module`
- **display_name**: `Template Module`

## What this module is for

This is a starter department. Replace this with a short description of what your module actually does.

## Inputs this module expects

- Replace with the information your module needs from the user
- Replace with any files or artifacts your module expects

## Outputs this module produces

- Replace with the artifacts or decisions your module returns
- Replace with the final state a user should expect

## Suspend rundown template

> **Status:** current state in one sentence
> **What changed:** short summary of recent progress
> **Open questions:** anything still unresolved
> **Next action:** the next useful move
> **Artifacts:** files or outputs to revisit

## Portable bridge note

This starter uses the optional `bridge/status.json` contract so Chatty-EDU can read what happened without taking ownership of the module's real UI/state.

If you want portable lesson/workflow mirroring too, add:
- `bridge/shared_state.json` for the state your module wants to share outward
- `bridge/incoming_shared_state.json` for mirrored state the host delivers back in
