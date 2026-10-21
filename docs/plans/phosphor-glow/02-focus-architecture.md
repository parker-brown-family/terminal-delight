# Architecture

The existing Grade and channel ownership model carries optional inactive_image_glow, inactive_text and afterglow_ms alongside image_glow. Missing fields remain absent on disk and in raw MCP reports. Presentation accessors supply 15%, 65%, 75% and 180 ms. is_default must compare every new optional field, including the original image_glow, to prevent an edited default grade from being omitted during saving.

TerminalView reads actual pane focus and applies the inactive multiplier only to the grid's text grade. Headers, background, documents and workbench colours keep their existing treatment. The GPU bloom receives the selected focus strength even when the glass is flat/off. Glass-only effects retain the CRT switch.

Afterglow reuses the existing retained GridPaint viewport. A bounded collection of changed graphemes keeps their old colour and font, without cell backgrounds or cursor/selection decorations. The grid draws those old glyphs with decreasing alpha while displaying the new styled rows immediately. The existing GPU bloom includes their light. No extra framebuffer, history texture or GPU patch is required.

History belongs to one GridPaint entity. Geometry, font/theme, viewport scroll, alternate-screen, covered-face and tab-visibility changes clear it. A finite time and glyph budget bound its lifetime and memory. Animation requests stop when the collection is empty. Use the GPUI executor clock so tests can advance time deterministically.

The already installed unicode-segmentation and unicode-width dependencies provide grapheme boundaries and terminal column widths; declare them directly rather than implementing Unicode tables.
