# Omarchy's stock themes

Each `<id>.toml` here is `/usr/share/omarchy/themes/<id>/colors.toml` from Omarchy 4.0.4, copied verbatim. Omarchy is MIT-licensed (`LICENSE`, from github.com/basecamp/omarchy).

Terminal Delight compiles these in (`app/src/palette.rs`, `STOCK`) so a machine with no Omarchy still opens on the same themes. Where Omarchy is installed, its own copies are read instead and these are never offered.

To refresh after Omarchy changes a theme, copy the new file over the old one; the test `the_stock_copies_match_the_desktop_they_came_from` names every file that has drifted:

```bash
for f in app/palettes/omarchy/*.toml; do cp "/usr/share/omarchy/themes/$(basename "$f" .toml)/colors.toml" "$f"; done
```
