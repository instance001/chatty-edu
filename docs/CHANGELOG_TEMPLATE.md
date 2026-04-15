# Changelog Template

Use this when a module is going to have more than one release and you want a simple running history file.

This is the long-term companion to:

- `chatty-edu/docs/MODULE_RELEASE_NOTES_TEMPLATE.md`

Rule of thumb:

- release notes = one release announcement
- changelog = the running history over time

## Copy-and-fill template

```text
# Changelog

All notable changes to this module will be listed here.

## [Unreleased]

### Added
- ...

### Changed
- ...

### Fixed
- ...

### Removed
- ...

## [1.0.0] - YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...

## [0.9.0] - YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...
```

## Recommended sections

Use only the sections you need:

- `Added`
- `Changed`
- `Fixed`
- `Removed`

That is enough for most modules.

## How to keep it healthy

Recommended:

- add new work to `Unreleased` first
- when shipping, move those items into a dated version section
- keep entries user-facing where possible
- keep wording short and concrete

Avoid:

- giant internal dev diaries
- entries like "misc stuff"
- mixing future work and shipped work in the same section

## Good example

```text
# Changelog

All notable changes to this module will be listed here.

## [Unreleased]

### Added
- Drafted budget-planning presets for grocery mode

## [0.3.0] - 2026-03-27

### Added
- Added a grocery snapshot section
- Added clearer weekly meal overview cards

### Changed
- Improved hosted Chatty-EDU layout spacing

### Fixed
- Fixed a reset issue after tab reload
```

## Suggested pairing

This template works best alongside:

- `README.md`
- `USER_MANUAL.md`
- `chatty-edu/docs/MODULE_RELEASE_NOTES_TEMPLATE.md`
- `chatty-edu/docs/MODULE_PACKAGING_GUIDE.md`
- `chatty-edu/docs/MODULE_REVIEW_RUBRIC.md`
- `chatty-edu/docs/MODULE_SUBMISSION_TEMPLATE.md`
