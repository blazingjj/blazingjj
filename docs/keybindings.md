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

### Global

These work in every tab, and `scroll-down` and `scroll-up` scroll the
popups as well. Selecting a tab by its number in the tab bar (`1` to `4`)
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

```toml
[blazingjj.keybinds.log-tab]
mark-head = "space"
goto-parent = "-"

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

push = "p"
push-new = "ctrl+p"
push-all = "shift+p"
push-all-new = "ctrl+shift+p"
fetch = "f"
fetch-all = "shift+f"
```

### Files tab

```toml
[blazingjj.keybinds.files-tab]
untrack = "x"
restore = "r"
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
