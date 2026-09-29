# Optional desktop launcher

The repository provides [`omarchy-buzz.desktop`](../applications/omarchy-buzz.desktop)
so an application launcher can find **Buzz** and open its normal, resizable
window. It sends the existing Omarchy shell command with `{"mode":"window"}`;
the desktop entry starts no separate chat process. The Buzz plugin must already
be installed and enabled in the running Omarchy shell.

Install it from the repository checkout only if the destination is unused:

```sh
target="$HOME/.local/share/applications/omarchy-buzz.desktop"
mkdir -p "$HOME/.local/share/applications"
test ! -e "$target" && test ! -L "$target" &&
  install -m 644 applications/omarchy-buzz.desktop "$target"
```

An existing file or symlink is left alone. To remove this optional launcher,
compare it to the repository copy first so a changed entry is preserved:

```sh
target="$HOME/.local/share/applications/omarchy-buzz.desktop"
test ! -L "$target" && cmp -s applications/omarchy-buzz.desktop "$target" &&
  rm -- "$target"
```

The entry uses `omarchy-shell shell summon community.buzz` with one JSON
argument. Desktop entries do not invoke a shell for `Exec`; the escaped quotes
are parsed as part of that single argument. You can check the file before
installing it with `desktop-file-validate applications/omarchy-buzz.desktop`.
No menu cache update or shell restart is needed for the entry itself.
