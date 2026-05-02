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
