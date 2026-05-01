## Configuring keybindings

```toml
# change keybinding
save = "ctrl+s"
# set multiple keybindings
save = ["ctrl+s", "ctrl+shift+g"]
# disable keybinding
save = false
```

In below examples default values are used.

### Top-level scroll bindings

These apply as defaults to all scroll-capable components and can be overridden
in each component's own section.

```toml
[blazingjj.keybinds]
scroll-down = ["j", "down"]
scroll-up = ["k", "up"]
scroll-down-half = "shift+j"
scroll-up-half = "shift+k"

toggle-layout = "ctrl+w"
```

### Message popup

Overrides top-level scroll bindings. `scroll-down-page` and `scroll-up-page`
are only configurable here.

```toml
[blazingjj.keybinds.message-popup]
scroll-down = ["j", "down"]
scroll-up = ["k", "up"]
scroll-down-half = "ctrl+d"
scroll-up-half = "ctrl+u"
scroll-down-page = ["ctrl+f", "space", "pagedown"]
scroll-up-page = ["ctrl+b", "pageup"]
```

### Log tab

```toml
[blazingjj.keybinds.log-tab]
save = "ctrl+s"
cancel = "esc"

close-popup = "q"

scroll-down = ["j", "down"]
scroll-up = ["k", "up"]
scroll-down-half = "shift+j"
scroll-up-half = "shift+k"

focus-current = "@"
toggle-diff-format = "w"

refresh = ["shift+r", "f5"]
create-new = "n"
create-new-describe = "shift+n"
duplicate = "shift+d"
squash = "s"
squash-ignore-immutable = "shift+s"
edit-change = "e"
edit-change-ignore-immutable = "shift+e"
abandon = "a"
absorb = "shift+a"
describe = "d"
edit-revset = "r"
set-bookmark = "b"
open-files = "enter"
copy-change-id = "y"
copy-rev = "shift+y"

push = "p"
push-new = "ctrl+p"
push-all = "shift+p"
push-all-new = "ctrl+shift+p"
fetch = "f"
fetch-all = "shift+f"

open-help = "?"
```

### Mouse: drag-and-drop in the log tab

Press and hold the left mouse button on a commit, drag onto another commit,
and release to move or squash. Modifiers held at release pick the operation:

| Modifier on release | Operation                                            |
| ------------------- | ---------------------------------------------------- |
| (none)              | `jj rebase -r <source> -d <target>` (rebase onto)    |
| `Alt`               | `jj rebase -r <source> -A <target>` (insert after)   |
| `Ctrl`              | `jj rebase -r <source> -B <target>` (insert before)  |
| `Shift`             | `jj squash --from <source> --into <target>`          |

All paths use `-r` (single revision): only the dragged commit moves, and its
descendants are reparented to skip it. Use the keybind-driven rebase popup
(`r`) when you want to bring descendants along (`-s`) or rebase the whole
branch (`-b`).

Shift+click is intercepted by most terminals for native text selection, so
the squash gesture is only available in terminals that forward `Shift`
(e.g. kitty).

If the dragged commit is part of a marked set (`Space` toggles marks), the
whole marked set is used as the source.

A footer under the log panel shows the legend and the abbreviated source and
target change ids while a drag is in flight. Press `Esc` to cancel.

A bare click without movement still selects the commit, as before.
