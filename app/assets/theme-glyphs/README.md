# Theme glyphs

One icon per Omarchy theme, drawn in that theme's own accent on its own background in the theme tray (`app/src/glyph.rs` maps theme id to file).

The icons are Material Design Icons by Pictogrammers, from github.com/Templarian/MaterialDesign-SVG, redistributed under the Apache License 2.0 (`LICENSE`). Each file is the upstream SVG unchanged; gpui draws it as a mask, so its own colour is ignored and the theme's is used.
