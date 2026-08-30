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

focus-current = "@"
refresh = ["shift+r", "f5"]
open-help = "?"

next-tab = "l"
prev-tab = "h"

command-popup = ":"
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

### Log tab

```toml
[blazingjj.keybinds.log-tab]
toggle-diff-format = "w"

goto-parent = "-"

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
