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

These work in every tab. The scroll bindings also apply as defaults to the
popups, which can override them in their own section. Selecting a tab by its
number in the tab bar (`1`, `2`, `3`) is not configurable.

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
quit = ["q", "ctrl+c", "esc"]
```

### Message popup

Overrides the global scroll bindings. `scroll-down-page` and `scroll-up-page`
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
copy-change-id = "y"
copy-rev = "shift+y"

push = "p"
push-new = "ctrl+p"
push-all = "shift+p"
push-all-new = "ctrl+shift+p"
fetch = "f"
fetch-all = "shift+f"
```
