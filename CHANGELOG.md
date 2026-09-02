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
  `focus-current`, `refresh`, `open-help` and `open-context-menu` move there
  from `[blazingjj.keybinds.log-tab]`, which also loses its `scroll-*` overrides
- jj 0.42.0 or newer is now required
- The confirmation dialogs are now popups of the app and go away on `q` or
  Escape, like the other popups, rather than on the log tab's `close-popup`
  and `cancel` bindings. `[blazingjj.keybinds.log-tab] close-popup` and
  `cancel` bound nothing else and are gone
- The keys every popup answers to are now configured under
  `[blazingjj.keybinds.popup]` (`accept`, `cancel` and the `scroll-*-half`
  and `scroll-*-page` bindings) and `[blazingjj.keybinds.text-popup]` (`accept`,
  `cancel`), rather than being the log tab's `save` and `cancel` or not
  configurable at all. `[blazingjj.keybinds.log-tab] save` is gone with them
- A popup now scrolls a line at a time by the global `scroll-down` and
  `scroll-up`, which it can no longer override, and by half a page and by a
  page the way the details panel does: `[blazingjj.keybinds.message-popup]`
  is gone
- A popup showing a message or the help no longer goes away on `y`, `n` or
  `o`, only on what accepts or cancels a popup
- Escape no longer quits the app; `q` and `Ctrl+c` still do, and
  `[blazingjj.keybinds] quit` can bind it back
- The details panel is now configured under `[blazingjj.keybinds.details-panel]`
  and answers to the same keys in every tab. `[blazingjj.keybinds.log-tab]
  toggle-diff-format`, which only ever reached the log tab's panel, is gone
- The push targets are reached from the push menu on `p` and are no longer
  bound to keys of their own: `[blazingjj.keybinds.log-tab] push`, `push-new`,
  `push-all` and `push-all-new` are still there and can bind `p`, `ctrl+p`,
  `shift+p` and `ctrl+shift+p` back

### Added

- Text in the details panel can be marked by dragging the mouse over it,
  by double clicking a word or by triple clicking a line, and goes to the
  system clipboard when the button comes up. A line the panel wrapped or
  cut off to show is copied as the one whole line it is
- `o` in the files tab opens the selected file, as the working copy has it,
  in an editor, which `blazingjj.editor` names and `blazingjj.editor-mode`
  says whether to hand the terminal to or leave running on its own. Without
  one configured, `$VISUAL` and then `$EDITOR` are used
- Diff format rendering the Git format with a pager like
  [delta](https://github.com/dandavison/delta), configured as
  `blazingjj.diff-pager` and toggled through with `w` like the others
- Push menu (`p`, `push-menu`, or `Push` in the log tab's context menu) listing
  what a push can send, each target on a key of its own inside the menu
- The push menu can create the bookmark it sends, either named after the
  change the way jj's `templates.git-push-bookmark` says (`c`) or under a name
  you give (`n`)
- A push now says what it would do and asks before it sends anything;
  `blazingjj.confirm-push = false` pushes right away as before
- The details panel now names the diff format it renders in, in its top
  right corner
- Operation log tab, listing what the repo has been through and showing what
  the selected operation did to it; opened from the tab bar with `5`. It reads
  the newest 200 operations, `m` reads twice as far back, and `Y` yanks the
  selected operation's id
- Keybindings in the operation log tab to restore the repo to the selected
  operation (`r`, `jj op restore`) and to take that one operation back (`v`,
  `jj op revert`), both after a confirmation
- Settings tab, listing the `blazingjj.*` options with what the configuration
  says about them and what the app goes by while it says nothing; opened from the
  tab bar with `0`, where it sits after the tabs that show the repo. `Enter`
  changes the selected setting and `x` takes it back out of your config, both
  through `jj config` on the user config file and both configurable under
  `[blazingjj.keybinds.settings-tab]`. A change takes effect at once, without
  a restart
- Keybindings tab, opened from the settings tab's `blazingjj.keybinds` row,
  listing every action a key can be bound to under the heading of where its
  keys take effect. `Enter` takes the next key you press for the selected
  action and `a` takes one more key beside the keys it has, `X` leaves it bound
  to nothing and `x` takes the binding back out of your config; `Esc` goes back
  to the settings
- Keybinding in the bookmarks tab to point a bookmark at the change the
  selected line stands for (`b`, as in the log tab), which settles a bookmark
  torn between several targets on the one selected, and moves a bookmark to
  what one of its remotes has
- Evolog tab, listing the versions a change has had and showing what the rewrite
  that produced the selected one changed; opened for the change selected in the
  log tab with `v` (`open-evolog`), or from the tab bar with `4`. A version's
  files open with `Enter`, `D` duplicates it as a new change, and `Y` yanks its
  revision
- The tabs overview takes the mouse: a click switches to the tab clicked, and
  the wheel cycles through the tabs
- The keys the confirmation, set-bookmark and rebase popups have of their own
  are now configurable, under `[blazingjj.keybinds.confirm-popup]`,
  `[blazingjj.keybinds.bookmark-set-popup]` and
  `[blazingjj.keybinds.rebase-popup]`. The first two mark the key a button or
  option is bound to in its label, or name it after the label where the label
  has no such letter, rather than marking a letter that a rebinding would move
- `[blazingjj.keybinds.log-tab] mark-head` for marking a change, which had no
  binding to configure and no line in the help
- The files, bookmarks and evolog tabs can now have their keybindings
  configured, under `[blazingjj.keybinds.files-tab]`,
  `[blazingjj.keybinds.bookmarks-tab]` and `[blazingjj.keybinds.evolog-tab]`
- The details panel keybindings are now all configurable: `scroll-down`,
  `scroll-up`, the `scroll-*-half` and `scroll-*-page` bindings, `toggle-wrap`
  and `toggle-diff-format`
- The help popup now scrolls with the mouse wheel
- The help popup now shows a scrollbar when it does not fit on screen
- Keybinding for jj absorb (`A`)
- Going to the top or bottom of the list (`Ctrl+Home` and `Ctrl+End`) now works
  in the files, bookmarks and evolog tabs as well, and is configured under
  `[blazingjj.keybinds]` as `scroll-to-top` and `scroll-to-bottom` alongside
  the other ways of scrolling
- Global keybindings under `[blazingjj.keybinds]` (`scroll-down`, `scroll-up`,
  `scroll-down-half`, `scroll-up-half`, `focus-current`, `refresh`, `open-help`,
  `next-tab`, `prev-tab`, `open-context-menu`, `command-popup`,
  `interactive-command-popup`, `quit`) that work the same in every tab;
  `scroll-down` and `scroll-up` scroll the popups as well
- The help popup now lists the global keybindings alongside the main and details panel ones
- Message popup now supports scrolling with a scrollbar
- Command popup output now preserves ANSI color
- Drag to resize pane divider in all tabs
- Mouse scrolling in the file and bookmark list panes
- Left-click to select items in all list panes; log tab click now fires on press rather than release
- A double click on a change in the log tab marks it, as `space` does
- The dedicated `Menu` key can now be used in keybindings, spelled `menu`
- Keybinding to move the log tab selection to the parent commit (`-`), asking
  which one when a merge has more than one parent in the log view; parents
  outside it are listed alongside but cannot be selected. The popup takes the
  mouse as well: the wheel scrolls it, a click picks a parent, and a click
  outside dismisses it
- The new-change dialog now asks where to put the change, so it can also be
  inserted before or after the selected one rather than only as its child; the
  bookmarks tab asks the same instead of only confirming
- Work done outside the app, such as a jj command run in another terminal, is
  picked up on its own while the terminal window has no focus
- `blazingjj.poll-interval` to set how often the app checks for it, or `0` to
  have it only check when asked
- The header's `R: refresh` hint turns red when the current tab has gone
  stale and the app is not going to catch it up on its own
- A right click now takes a choice popup away: one inside it cancels, one
  outside also goes on to the tab underneath, which may act on what was hit
- Context menu popup (right-click or `Menu` key) with the common operations for
  whatever the tab has selected: a change, a file, a version or a bookmark
- Toggle horizontal/vertical split at runtime with the `toggle-layout` keybind,
  which comes unbound
- `blazingjj.describe-mode` to set what describing a change puts up, the
  built-in editor or an interactive `jj describe`
- `!` opens the command popup with the terminal handed to what is run, so
  that commands wanting an editor, such as `describe` or `split`, can be
  used; running one with `:` offers to run it in the terminal rather than
  hanging

### Changed

- The tab bar now scrolls the current tab into the middle of what it shows
  when the window is too narrow for every tab, rather than cutting off the
  tabs that do not fit. The header no longer says which numbers pick a tab,
  which the tab bar shows for itself
- Clicking a change in the log tab no longer scrolls the log to put the
  selection into the middle of the panel; the line clicked stays where it is
- The set-bookmark dialog now offers the bookmarks the change is standing on
  first, nearest first, rather than ordering them by how recently the change
  each points at was committed
- The files tab now reads a change's files when it comes on screen, so an
  operation no longer runs `jj` for it while another tab is up
- The popups now all scroll alike, so the lists and the help scroll by half a
  page on `Ctrl+d` and `Ctrl+u` and by a page on `Ctrl+f` and `Ctrl+b` as well,
  as the message popup already did
- A popup's own keys are now matched after the ones every popup answers to,
  which is what the set-bookmark and confirmation popups already did and the
  rebase popup did the other way round
- The line under a popup naming the keys it answers to now names the keys
  they are bound to, rather than the ones they were bound to when it was
  written
- The help popup now lists every keybinding under a heading for what it does
  rather than for who binds it: the keys that move around in the main panel --
  scrolling, `@`, going to the top or bottom of the list, and the context
  menu -- join the panel's navigation heading rather than the global
  keybindings, and the files and evolog tabs get a heading per kind of thing
  their keys do rather than one holding all of them
- The help popup now lists a tab's main panel keybindings under a heading per
  kind of thing they do -- navigation, changes, bookmarks and remotes,
  clipboard -- rather than as one run, and spreads those headings over a second
  column where the terminal is wide enough to hold one
- The help popup is now sized to fit what it lists
- The message popup is now sized to fit its message, rather than always
  taking up most of the screen
- The help popup now lists the keybindings next to what they do rather than in
  front of it
- Pressing `s` on the working copy now offers to squash into the parent (when there is exactly one)
- In log and bookmarks tab, the details panel update no longer blocks the UI
  thread, and the changes it has shown are cached
- Scrolling past a change no longer abandons the `jj show` running for it, so
  coming back to it is instant
- In files tab, the diff panel update no longer blocks the UI thread, and the
  diffs it has shown are cached
- The details panel now runs ahead of the selection, so moving onto one of the
  next few entries of the list shows their content without a wait

### Fixed

- `@` in the bookmarks tab now goes to the bookmark on the working copy, or to
  the one it is standing on, rather than doing nothing while the help offers it
- A keybinding on Home, End or the space bar now shows the key's name rather
  than "Unknown" or a blank
- The help popup now lists the key that opens the context menu in every tab,
  the bookmarks tab included
- An operation jj turns down no longer takes the app down with it: the reason
  goes up in a popup, as it already did for the operations refused before they
  were run
- A duplicate jj turns down now says so, rather than looking like it worked
- A file that cannot be untracked now reports what jj said about it, rather
  than guessing that it needs to be ignored
- The bookmarks tab now lists a conflicted bookmark's targets under it, the
  way `jj bookmark list` does, rather than only saying that it is conflicted;
  selecting one shows that change in the details panel
- A conflicted bookmark is now offered by the set-bookmark dialog, which is
  where it would be pointed at a single change to resolve it
- Switching tabs right after an operation no longer briefly shows what they
  held before it
- A describe jj turns down no longer loses what was written: the editor stays
  up with the description and what jj said about it
- Going to the current change (`@`) now shows it right away, rather than
  leaving the tab on the change it was on until the view is refreshed by hand
- Creating a change from the bookmarks tab now brings the view up to date,
  rather than leaving it to the next poll
- A bookmark name the set-bookmark dialog cannot put on a change is no longer
  lost: the dialog comes back with the name and what jj said about it
- Editing the change a bookmark points at now brings the other tabs up to date,
  as editing one from the log does
- `d` in the bookmarks tab no longer offers to delete a bookmark that is not
  there to be deleted
- What jj said about a bookmark name it turned down now wraps in the popup that
  puts the question back, rather than being cut off at the edge
- The help popup no longer drops the global keybindings when the terminal is too
  short to hold them next to the details panel ones
- The help popup now scrolls its three keybinding lists as a whole rather than
  each of them within its own box
- The help popup no longer cuts the last two characters off its longest
  descriptions
- The help popup now stacks all three keybinding lists in a single column when
  the terminal is too narrow to hold two of them side by side
- Describing a commit with a message starting with a dash no longer fails
- A copied file in the files list is now colored like the other changes
- Running a command from the log tab no longer moves the selection to the
  working copy
- In files tab, a file renamed within a directory now shows its diff, and `x`/`r`
  no longer act on a path that does not exist
- Conflicted paths in the files list no longer carry trailing spaces when a
  change has conflicts in paths of differing lengths
- Coming back to the terminal window no longer stalls the UI while `jj log` runs
  again for a repo that has not changed
- Resizing the details panel no longer empties it while an external diff tool
  renders the change again
- With `diff-format = "stat"`, the histogram is now scaled to the panel rather
  than to the whole terminal
- Pushing new bookmarks works again: `ctrl+p` now names the bookmarks on the
  selected change, which is what makes jj track them, and says so when the
  change has none. `ctrl+shift+p` pushes all bookmarks and `shift+p` only the
  tracked ones, as they did before

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
