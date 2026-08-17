# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Breaking Changes

- The keybinds config section is now kebab-cased: `[blazingjj.keybinds.log_tab]` must be
  changed to `[blazingjj.keybinds.log-tab]`
- The keybindings shared by all tabs are now configured under `[blazingjj.keybinds]`:
  `focus-current`, `refresh` and `open-help` move there from
  `[blazingjj.keybinds.log-tab]`, which also loses its `scroll-*` overrides
+- jj 0.37.0 or newer is now required

### Added

- Keybinding for jj absorb (`A`)
- Global keybindings under `[blazingjj.keybinds]` (`scroll-down`, `scroll-up`,
  `scroll-down-half`, `scroll-up-half`, `focus-current`, `refresh`, `open-help`,
  `next-tab`, `prev-tab`, `command-popup`, `quit`) that work the same in every
  tab; the scroll ones also apply as defaults to the popups, which can override
  them per-component
- The help popup now lists the global keybindings alongside the main and details panel ones
- Message popup now supports scrolling with a scrollbar
- Command popup output now preserves ANSI color
- Drag to resize pane divider in all tabs

### Changed

- Pressing `s` on the working copy now offers to squash into the parent (when there is exactly one)
- In log tab, the details panel update no longer blocks the UI thread.

### Fixed

- Describing a commit with a message starting with a dash no longer fails
- A copied file in the files list is now colored like the other changes
- Running a command from the log tab no longer moves the selection to the
  working copy
- In files tab, a file renamed within a directory now shows its diff, and `x`/`r`
  no longer act on a path that does not exist
- Conflicted paths in the files list no longer carry trailing spaces when a
  change has conflicts in paths of differing lengths

## [0.8.0] - 2026-04-19

### Added

- Keybinding for jj duplicate
- Log panel can mark and abandon multiple commits
- Log panel create new revision with marked commits as parents
- Add support for copying the Change ID/revision of the current log tab entry using y/Y
- Fix Describe dialog width at git recommendation for commit message
- Log tab diff is cached
- Process multiple events per frame
- Go to top and bottom of visible log

### Fixed

- prevent (macos) os error 22 crash by capping event poll timeout

## [0.7.1] - 2026-01-16

### Fixed

 - Avoid unnecessary redraws on mouse move events which caused massive CPU spikes


## [0.7.0] - 2026-01-13

### Added

- Details panel responds to mouse scroll in all tabs
- Details panel sets COLUMNS to allow jj diff tool to fit window
- Update the details panel when gaining focus
- Added an animated popup for fetch/push operations

### Changed

- Move from bookmark-prefix to bookmark-template for the bookmark generation to match the behaviour from jj 0.31+
- Fork project and change name from "lazyjj" to "blazingjj"

### Removed

- The Command log tab

<!-- next-url -->
[Unreleased]: https://github.com/blazingjj/blazingjj/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/blazingjj/blazingjj/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/blazingjj/blazingjj/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/blazingjj/blazingjj/compare/v0.6.1...v0.7.0
