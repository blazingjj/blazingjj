# Context menus

The context menu of a tab (`Menu`, or a right click) lists what can be done
to what the tab has selected. What each menu holds and the order it holds it
in is `blazingjj.context-menu`, one key per menu:

```toml
[blazingjj.context-menu]
log = ["edit-change", "describe", "abandon", "copy-change-id"]
```

A menu the configuration says nothing about holds every item the app comes
with, in the order listed below. A menu that is configured holds what it
lists and nothing else, so `defaults` stands for every item the app comes
with and saves spelling them all out again:

```toml
[blazingjj.context-menu]
# Everything the app has, with your own commands after it
log = ["defaults", "create-pr"]
# Everything the app has, with the copying moved to the front
evolog = ["copy-rev", "defaults"]
```

An item is named by the same name as the keybinding that runs it, so an
action goes by one name however it is reached;
[keybindings.md](keybindings.md) says which key each of them answers to. A
name that is listed twice puts the item in the menu once, where it is first
asked for.

An item that cannot be done to what is selected is left out, whether or not
it is asked for: the log's `rebase` is not offered on the change the working
copy is already on, and a bookmarks tab line that is no bookmark offers
nothing but `create-bookmark`. A name that is no item at all is listed under
the menu in red, there being nothing else to say about a name that is set but
does not read.

The settings tab (`0`) does the same from within blazingjj, on the
`blazingjj.context-menu` row.

## What each menu can hold

### Log tab

```toml
[blazingjj.context-menu]
log = [
  "edit-change",
  "create-new",
  "create-new-describe",
  "describe",
  "absorb",
  "abandon",
  "duplicate",
  "squash",
  "rebase",
  "push-menu",
  "set-bookmark",
  "copy-change-id",
  "copy-rev",
]
```

### Files tab

```toml
[blazingjj.context-menu]
files = ["open", "restore", "untrack"]
```

### Bookmarks tab

```toml
[blazingjj.context-menu]
bookmarks = [
  "create-bookmark",
  "rename-bookmark",
  "delete-bookmark",
  "forget-bookmark",
  "track-bookmark",
  "untrack-bookmark",
  "edit-change",
  "create-new",
  "create-new-describe",
  "view-in-log",
]
```

`track-bookmark` and `untrack-bookmark` only apply to a bookmark on a remote.

### Evolog tab

```toml
[blazingjj.context-menu]
evolog = ["open-files", "duplicate", "copy-rev"]
```

### Operation log tab

```toml
[blazingjj.context-menu]
op-log = ["restore", "revert", "copy-id"]
```
