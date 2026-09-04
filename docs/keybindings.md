## Configuring keybindings

```toml
# change keybinding
describe = "d"
# set multiple keybindings
describe = ["d", "ctrl+shift+g"]
# disable keybinding
describe = false
```

In below examples default values are used.

The keybindings tab does the same from within blazingjj: the settings tab
(`0`) opens it on the `blazingjj.keybinds` row, and it lists every action
under the heading of where its keys take effect. A key is written under
the name it reads by, so what the app shows is also what the config file
takes; a key it has no name for, such as one of the lock keys, is one
it cannot offer. `Enter` takes the key you press next as the one key an
action answers to, `a` takes it as another key beside the ones it has.

### Global

These work in every tab, and `scroll-down` and `scroll-up` scroll the
popups as well. Selecting a tab by its number in the tab bar (`0` to `6`)
is not configurable.

```toml
[blazingjj.keybinds]
scroll-down = ["j", "down"]
scroll-up = ["k", "up"]
scroll-down-half = "shift+j"
scroll-up-half = "shift+k"
scroll-to-top = "ctrl+home"
scroll-to-bottom = "ctrl+end"

focus-current = "@"
refresh = ["shift+r", "f5"]
# toggle-layout comes unbound, so this is an example rather than a default
toggle-layout = "ctrl+w"
open-help = "?"

next-tab = "l"
prev-tab = "h"

open-context-menu = "menu"
command-popup = ":"
interactive-command-popup = "!"
quit = ["q", "ctrl+c"]
```

### Popups

These work in every popup, and are left out of the help, which lists what
the tab under the popup answers to. A popup scrolls a line at a time by
the global `scroll-down` and `scroll-up`, and by half a page and by a
page the way the details panel does. A popup holding a text field takes
every key the field can take, so it has bindings of its own; the field of
a single line, having no newline to put an Enter in, accepts on Enter as
well.

```toml
[blazingjj.keybinds.popup]
accept = "enter"
cancel = ["esc", "q"]

scroll-down-half = "ctrl+d"
scroll-up-half = "ctrl+u"
scroll-down-page = ["ctrl+f", "space", "pagedown"]
scroll-up-page = ["ctrl+b", "pageup"]

[blazingjj.keybinds.text-popup]
accept = "ctrl+s"
cancel = "esc"
```

Three popups have keys of their own besides, for the buttons and options
they put up. Those are matched after the keys every popup answers to, so
binding one of them to a key a popup already uses leaves it unreachable.
The confirmation popup and the set-bookmark popup mark these keys in the
label of the button or option they press, or name them after it where the
label has no such letter, so a rebinding shows up there.

```toml
[blazingjj.keybinds.confirm-popup]
yes = "y"
no = "n"
select-yes = "left"
select-no = "right"

[blazingjj.keybinds.bookmark-set-popup]
use-generated-name = "g"
create-bookmark = "c"

[blazingjj.keybinds.rebase-popup]
source-with-descendants = "s"
source-whole-branch = "b"
source-single-revision = "r"
target-new-branch = "d"
target-insert-after = "shift+a"
target-insert-before = "shift+b"
```

### Details panel

These work in the details panel of every tab.

```toml
[blazingjj.keybinds.details-panel]
scroll-down = "ctrl+e"
scroll-up = "ctrl+y"
scroll-down-half = "ctrl+d"
scroll-up-half = "ctrl+u"
scroll-down-page = "ctrl+f"
scroll-up-page = "ctrl+b"

toggle-diff-format = "w"
toggle-wrap = "shift+w"
```

### Log tab

`use-marks` is a prefix: the operation that follows it acts on the marked
changes rather than on the selected one.

```toml
[blazingjj.keybinds.log-tab]
mark-head = "space"
use-marks = ";"

cancel = "esc"

goto-parent = "-"
goto-child = "+"

create-new = "n"
create-new-describe = "shift+n"
duplicate = "shift+d"
rebase = "ctrl+r"
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
open-evolog = "v"
copy-change-id = "y"
copy-rev = "shift+y"

push-menu = "p"
# The targets the push menu offers, none of them on a key of its own
push = false
push-new = false
push-all = false
push-all-new = false
push-change = false
push-named = false
fetch = "f"
fetch-all = "shift+f"
```

### Files tab

```toml
[blazingjj.keybinds.files-tab]
untrack = "x"
restore = "r"
open = "o"
```

### Bookmarks tab

```toml
[blazingjj.keybinds.bookmarks-tab]
toggle-show-all = "a"

create-bookmark = "c"
rename-bookmark = "r"
delete-bookmark = "d"
forget-bookmark = "f"
track-bookmark = "t"
untrack-bookmark = "shift+t"
set-bookmark = "b"

view-in-log = "enter"
create-new = "n"
create-new-describe = "shift+n"
edit-change = "e"
edit-change-ignore-immutable = "shift+e"
```

### Evolog tab

```toml
[blazingjj.keybinds.evolog-tab]
open-files = "enter"
duplicate = "shift+d"
copy-rev = "shift+y"
```

### Operation log tab

```toml
[blazingjj.keybinds.op-log-tab]
load-more = "m"

restore = "r"
revert = "v"

copy-id = "shift+y"
```

### Workspaces tab

```toml
[blazingjj.keybinds.workspaces-tab]
switch = "enter"

add = "a"
rename = "r"
forget = "x"
```

### Settings tab

```toml
[blazingjj.keybinds.settings-tab]
change = "enter"
unset = "x"
```

### Keybindings tab

```toml
[blazingjj.keybinds.keybindings-tab]
bind = "enter"
bind-besides = "a"
disable = "shift+x"
unset = "x"
back = "esc"
```

### Commands tab

```toml
[blazingjj.keybinds.commands-tab]
change-command-line = "enter"
change-label = "l"
toggle-interactive = "i"
add = "n"
unset = "x"
back = "esc"
```

### Context menus tab

```toml
[blazingjj.keybinds.menus-tab]
toggle = "enter"
move-up = "shift+k"
move-down = "shift+j"
unset = "x"
back = "esc"
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
