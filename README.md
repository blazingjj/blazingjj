<div class="title-block" style="text-align: center;" align="center">

# blazingjj - A TUI for [Jujutsu/jj](https://github.com/jj-vcs/jj)

<p><img title="blazingjj logo" src="docs/logo.png" width="320" height="320"></p>

Built in Rust with Ratatui. Interacts with `jj` CLI.

</div>

## Features

- Log
  - Scroll through the jj log and view change details in side panel
  - Create new changes from selected change with `n`, as its child or spliced
    in before or after it
  - Mark several changes with `Space` to make them the parents of a new change
  - Go to a change's parent with `-`
  - Edit changes with `e`/`E`
  - Describe changes with `d`
  - Abandon changes with `a`
  - Duplicate a change with `D`
  - Rebase a change with `Ctrl+r`, picking what moves and where it lands
  - Absorb a change's diff into its mutable ancestors with `A`
  - See different revset with `r`
  - Set a bookmark to selected change with `b`
  - Fetch/push with `f`/`p`
  - Squash current changes to selected change with `s`/`S`
  - Yank change ID/revision to the system clipboard with `y`/`Y`
  - See how a change evolved with `v`
- Files
  - View files in current change and diff in side panel
  - See a change's files from the log tab with `Enter`
  - View conflicts list in current change
  - Untrack a file with `x`, restore it with `r`
- Bookmarks
  - View list of bookmarks, including from all remotes with `a`
  - Create with `c`, rename with `r`, delete with `d`, forget with `f`
  - Track bookmarks with `t`, untrack bookmarks with `T`
  - Point a bookmark at the selected line's change with `b`
  - View a bookmark's change in the log with `Enter`
  - Create new change with `n`, edit change with `e`/`E`
- Evolog
  - View the versions a change has had and what the rewrite that produced the
    selected one changed
  - See a change's evolog from the log tab with `v`
  - View a version's files with `Enter`
  - Duplicate a version as a new change with `D`, to recover what a rewrite
    folded away
  - Yank a version's revision to the system clipboard with `Y`
- Details panel: toggle between color words and git diff with `w`, wrapping
  with `W`
  - Render the git diff with a pager like delta by configuring
    `blazingjj.diff-pager`
- Mouse: scroll the panels, click to select, drag the divider to resize, right
  click for the context menu
- Config: Configure blazingjj with your jj config
- Command box: Run jj commands directly in blazingjj with `:`
- Help: See all key mappings with `?`

## Setup

Make sure you have [`jj`](https://martinvonz.github.io/jj/latest/install-and-setup) 0.42.0 or newer installed first.

- With [`cargo binstall`](https://github.com/cargo-bins/cargo-binstall): `cargo binstall blazingjj`
- With `cargo install`: `cargo install blazingjj --locked` (may take a few moments to compile)
- With pre-built binaries: [View releases](https://github.com/blazingjj/blazingjj/releases)

To build and install a pre-release version: `cargo install --git https://github.com/blazingjj/blazingjj.git --locked`

## Configuration

You can optionally configure the following options through your jj config:

- `blazingjj.highlight-color`: Changes the highlight color. Can use named colors. Defaults to `#323264`
- `blazingjj.diff-format`: Change the default diff format. Can be `color-words`, `git`, `pager`, `summary` or `stat`. Defaults to `color_words`
  - If `blazingjj.diff-format` is not set but `ui.diff.format` is, the latter will be used
- `blazingjj.diff-tool`: Specify which diff tool to use by default
  - If `blazingjj.diff-tool` is not set but `ui.diff.tool` is, the latter will be used
- `blazingjj.diff-pager`: Specify a pager rendering the Git format diff, like [delta](https://github.com/dandavison/delta): `jj config set --user blazingjj.diff-pager '["delta", "--width=$width", "--line-numbers"]'`
  - The pager reads the diff on standard input and writes its rendering to standard output; it must not page, since blazingjj shows the output itself
  - `$width` in an argument stands for the columns the details panel has, which a pager that cannot find out for itself needs to be told
  - Setting it makes `pager` the default format, unless `blazingjj.diff-format` says otherwise, and adds it to what `w` toggles through
- `blazingjj.bookmark-template`: Change the bookmark name template for generated bookmark names. Defaults to `'push-' ++ change_id.short()`
  - If `blazingjj.bookmark-template` is not set but `templates.git_push_bookmark` is, the latter will be used
- `blazingjj.describe-mode`: What describing a change puts up. Can be `popup` (default) for the built-in editor, or `jj` to hand the terminal to `jj describe` and your own editor
- `blazingjj.layout`: Changes the layout of the main and details panel. Can be `horizontal` (default) or `vertical`
- `blazingjj.layout-percent`: Changes the layout split of the main page. Should be number between 0 and 100. Defaults to `50`
- `blazingjj.poll-interval`: Seconds between checks for work done outside the app. Set to `0` to only check when asked. Defaults to `1`
  - What is found is picked up while the terminal window has no focus; while it has focus, the header's `R: refresh` hint turns red instead, as refreshing what is being read moves it
  - A terminal that does not report focus changes counts as always focused, so there the hint is all you get

Example: `jj config set --user blazingjj.diff-format "color-words"` (for storing in [user config file](https://martinvonz.github.io/jj/latest/config/#user-config-file), repo config is also supported)

## Usage

To start blazingjj for the repository in the current directory: `blazingjj`

To use a different repository: `blazingjj --path ~/path/to/repo`

To start with a different default revset: `blazingjj -r '::@'`

To use a different `jj` binary: `blazingjj --jj-bin ~/bin/jj` (or set `JJ_BIN`)

To start even though the `jj` version check fails: `blazingjj --ignore-jj-version`

## Key mappings

See all key mappings for the current tab with `?`.

### Basic navigation

- Quit with `q` or `Ctrl+c`
- Change tab with `1`/`2`/`3`/`4` or with `h`/`l`
- Go to the current change with `@`
- Refresh the current tab with `R` or `F5`
- Scrolling in main panel
  - Scroll down/up by one line with `j`/`k` or down/up arrow
  - Scroll down/up by half page with `J`/`K`
- Scrolling in details panel
  - Scroll down/up by one line with `Ctrl+e`/`Ctrl+y`
  - Scroll down/up by a half page with `Ctrl+d`/`Ctrl+u`
  - Scroll down/up by a full page with `Ctrl+f`/`Ctrl+b`
- Change details panel diff format between color words (default) and Git (and a diff pager and diff tool if set) with `w`
- Toggle details panel wrapping with `W`
- Open the context menu for what the tab has selected with `Menu` or a right click
- Open a command popup to run jj commands using `:` (jj prefix not required, e.g. write `new main` instead of `jj new main`)

### Log tab

- View change files in files tab with `Enter`
- View how the change evolved in the evolog tab with `v`
- Go to the top/bottom of the visible log with `Ctrl+Home`/`Ctrl+End`
- Go to the highlighted change's parent with `-`, choosing which one when a
  merge has more than one in view
- Mark the highlighted change with `Space`, to give a new change several parents
- Display different revset with `r` (`jj log -r`)
- Create new change with `n`, choosing whether it becomes a child of the
  highlighted change or is spliced in before or after it
  (`jj new [--insert-before|--insert-after]`)
  - Create new change and describe with `N` (`jj new -m`)
- Edit highlighted change with `e` (`jj edit`)
  - Edit highlighted change ignoring immutability with `E` (`jj edit --ignore-immutable`)
- Abandon a change with `a` (`jj abandon`)
- Duplicate the highlighted change with `D` (`jj duplicate`)
- Rebase with `Ctrl+r` (`jj rebase`), choosing whether the change moves alone,
  with its descendants or as a whole branch, and whether it lands on the
  selected change or before or after it
- Absorb the highlighted change's diff into its mutable ancestors with `A` (`jj absorb --from`)
- Describe the highlighted change with `d` (`jj describe`)
  - Save with `Ctrl+s`
  - Cancel with `Esc`
- Set a bookmark to the highlighted change with `b` (`jj bookmark set`)
  - Scroll in bookmark list with `j`/`k`
  - Create a new bookmark with `c`
  - Use auto-generated name with `g`
- Squash current changes (in @) to the selected change with `s` (`jj squash`)
  - Squash current changes to the selected change ignoring immutability with `S` (`jj squash --ignore-immutable`)
- Yank the change ID with `y` and the revision with `Y`
- Git fetch with `f` (`jj git fetch`)
  - Git fetch all remotes with `F` (`jj git fetch --all-remotes`)
- Git push the tracked bookmarks on the highlighted change with `p` (`jj git push -r`)
  - Push its bookmarks the remote does not have yet as well with `Ctrl+p` (`jj git push --bookmark`)
  - Push all tracked bookmarks with `P` (`jj git push --tracked`)
  - Push all bookmarks, new ones included, with `Ctrl+P` (`jj git push --all`)

### Files tab

- Untrack the highlighted file with `x` (`jj file untrack`)
- Restore the highlighted file with `r` (`jj restore`)

### Bookmarks tab

- Show bookmarks with all remotes with `a` (`jj bookmark list --all`)
- Create a bookmark with `c` (`jj bookmark create`)
- Rename a bookmark with `r` (`jj bookmark rename`)
- Delete a bookmark with `d` (`jj bookmark delete`)
- Forget a bookmark with `f` (`jj bookmark forget`)
- Track a bookmark with `t` (only works for bookmarks with remotes) (`jj bookmark track`)
- Untrack a bookmark with `T` (only works for bookmarks with remotes) (`jj bookmark untrack`)
- Point the bookmark at the change the selected line stands for with `b`
  (`jj bookmark set`), which settles a conflicted bookmark on that change
- View the highlighted bookmark's change in the log tab with `Enter`
- Create a new change from the highlighted bookmark's change with `n`, choosing
  where it goes (`jj new`)
  - Create a new change and describe with `N` (`jj new -m`)
- Edit the highlighted bookmark's change with `e` (`jj edit`)
  - Edit the highlighted bookmark's change ignoring immutability with `E` (`jj edit --ignore-immutable`)

### Evolog tab

- View the highlighted version's files in the files tab with `Enter`
- Duplicate the highlighted version as a new change with `D` (`jj duplicate`)
- Yank the highlighted version's revision with `Y`

### Configuring

Keys can be configured

```toml
[blazingjj.keybinds.log-tab]
describe = "d"
```

See more in [keybindings.md](docs/keybindings.md)

## Related Projects

 * [blazingjj.nvim](https://opencommit.eu/sejo/blazingjj.nvim) -- A Neovim plugin that provides a floating window interface for blazingjj

## Development

### Setup

1. Install Rust and
2. Clone repository
3. Run with `cargo run`
4. Build with `cargo build --release` (output in `target/release`)
5. You can point it to another jj repo with `--path`: `cargo run -- --path ~/other-repo`

### Logging/Tracing

blazingjj has 2 debugging tools:

1. Logging: Enabled by setting `BLAZINGJJ_LOG=1` when running. Produces a `blazingjj.log` log file
2. Tracing: Enabled by setting `BLAZINGJJ_TRACE=1` when running. Produces `trace-*.json` Chrome trace file, for `chrome://tracing` or [ui.perfetto.dev](https://ui.perfetto.dev)

## Release process

Create a release commit using [cargo
release](https://github.com/crate-ci/cargo-release), e.g. `cargo release
minor`, then open a PR and after it has been merged, create a GitHub release
