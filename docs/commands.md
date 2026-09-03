# Commands of your own

`blazingjj.commands` adds commands of your own, each named by the name a
context menu holds it by and configured as the command line to run:

```toml
[blazingjj.commands]
show-marked = ["jj", "show", "$marked"]
```

Most commands have nothing to say beyond what to run, which is what the
command line on its own configures: the output is captured and put up in a
popup, and a menu holding it says its name. A command with more to it says
it alongside the command line:

```toml
[blazingjj.commands.create-pr]
command = ["gh", "pr", "create", "--head", "$bookmark"]
label = "Create PR"
interactive = true
```

- `command`: the program to run and the arguments to run it with. A program
  on its own is a command line of one word, so `tug = "tug"` runs `tug`. The
  program is run in the repo root, and is looked up on `PATH` as usual; it
  need not be jj
- `label`: what a menu holding the command says. Without one, the menu says
  the name the command is configured under
- `interactive`: whether the terminal is handed over to the command, so that
  it can page, colorize and prompt as it would outside the app, and held once
  it is done so that what it printed can be read. Defaults to `false`, which
  captures what the command writes and puts it up in a popup

An argument is a word of the configuration rather than a word of a shell, so
an argument that holds whitespace has to be written as one string in the
config file. Nothing is passed through a shell, so a pipeline or a redirect
has to be a command of its own: `["sh", "-c", "..."]`.

## Naming what is selected

The arguments can name what the tab the command is run from has selected, by
the same placeholders the command popup takes:

| Placeholder | What it stands for |
| --- | --- |
| `$selected`, `$s` | What the tab is about: the file in the files tab, the bookmark in the bookmarks tab, the operation in the operation log, the revision everywhere else |
| `$marked`, `$m` | The changes the log has marked, as the one revset naming them all |
| `$revision` | The revision the tab is on, whichever kind of thing it is about |
| `$file` | The selected file |
| `$bookmark` | The selected bookmark |
| `$operation` | The selected operation |

A placeholder is replaced inside the argument holding it, so `--rev=$s` names
the selection as much as `$s` on its own does, and what it stands for is one
argument however it reads. A revset can be written around one, as in `$s-`,
and `$$` is a `$` of its own.

A command naming something the tab has nothing of is refused rather than run
without it, so `$marked` with nothing marked says so rather than running
against everything or nothing.

`$revision` is the change id, so the command reads the change as it stands,
except that a version out of the evolog and a divergent change are named by
their commit id, those being what they are found by.

## Putting a command in a menu

A command goes in a context menu by listing its name in
`blazingjj.context-menu`, where `defaults` stands for every item the app
comes with:

```toml
[blazingjj.context-menu]
log = ["defaults", "create-pr"]
bookmarks = ["create-pr", "defaults"]
```

The app's own items come first, so a command that takes the name of one is
not picked in its place; [context-menus.md](context-menus.md) lists the names
every menu already has. A name that is neither an item nor a command of your
own is listed under the menu in red.

The settings tab (`0`) does the same from within blazingjj, on the
`blazingjj.commands` row.
