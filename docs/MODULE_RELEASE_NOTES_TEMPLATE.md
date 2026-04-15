# Module Release Notes Template

Use this when you want a simple, repeatable release note for a module update.

This is intentionally lightweight.

Good release notes should help another person quickly answer:

- what changed
- whether they should update
- whether they need to do anything differently

## Copy-and-fill template

```text
# <Module Name> Release Notes

Version: <x.y.z>
Release date: <YYYY-MM-DD>

## Summary

<One short paragraph. What is this release in plain language?>

## What's new

- ...
- ...
- ...

## Improvements

- ...
- ...

## Fixes

- ...
- ...

## Breaking or important changes

- None

or

- ...

## Install or update notes

- Drop the updated module folder into `chatty-edu/modules/`
- In Chatty-EDU, use **Modules -> Rescan modules**
- If this release needs a rebuild or new dependency, say it here

## Standalone notes

- <Does anything change for standalone users?>

## Hosted-in-Chatty-EDU notes

- <Does anything change for hosted use inside Chatty-EDU?>

## Known issues

- None known

or

- ...

## Recommended next step for users

- <Example: reopen the module and test the new planner flow>
```

## Short version for tiny updates

If the release is very small, this shorter format is enough:

```text
# <Module Name> Release Notes

Version: <x.y.z>
Release date: <YYYY-MM-DD>

## Summary

- Added ...
- Fixed ...
- Users should ...
```

## Writing tips

Recommended:

- lead with the user-visible change
- keep bullets concrete
- mention rebuild steps if they exist
- call out anything that changes install, launch, or compatibility

Avoid:

- giant internal dev diaries
- vague lines like "misc improvements"
- hiding breaking changes in the middle of a long list

## Good release note example

```text
# Meal Planner (Demo) Release Notes

Version: 0.3.0
Release date: 2026-03-27

## Summary

This update improves the hosted webview UI and gives Chatty-EDU a cleaner suspend handoff when you leave the tab.

## What's new

- Added a clearer weekly meal overview
- Added a grocery snapshot section

## Improvements

- Better layout inside hosted Chatty-EDU tabs
- Cleaner bridge summary text for cross-module context

## Fixes

- Fixed a state reset bug after tab reload

## Breaking or important changes

- None

## Install or update notes

- Replace the old module folder with the new one
- Use **Modules -> Rescan modules**

## Standalone notes

- Still works the same in standalone mode

## Hosted-in-Chatty-EDU notes

- Chatty-EDU now receives a better handoff summary on tab leave

## Known issues

- None known

## Recommended next step for users

- Reopen the module and save one planning pass to confirm the new handoff text
```

## Suggested companion files

This template works well alongside:

- `CHANGELOG.md`
- `README.md`
- `USER_MANUAL.md`
- `chatty-edu/docs/CHANGELOG_TEMPLATE.md`
- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md`
- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`
- `chatty-edu/docs/MODULE_SUBMISSION_TEMPLATE.md`
