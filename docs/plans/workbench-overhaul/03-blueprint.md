# Blueprint

Blueprint uses deep blue paper and pale annotation text. A pair of parallel rules marks the edge of a panel. Short perpendicular strokes mark the ends of a dimension line. Settings, help and the Floor use the same color roles as the workbench. Terminal cells keep their palette background. This preserves dark text on light terminal palettes such as Catppuccin Latte.

Chakra Petch supplies the interface text. Share Tech Mono supplies headings and data. Both faces ship with Terminal Delight. The skin uses the existing 22 px grid at 4.5% opacity. A dimension divider is a line and two borders. The skin uses the direct terminal renderer, with the animated tube and warp disabled. It requests zero glow, grain, frost and lift.

## Text contrast

The check reads the resolved skin and computes relative luminance after compositing the grid over each of five backgrounds, including a hover state. It measures plain paper, one grid line and the intersection of two lines. It checks the body, secondary, faint and inactive text, plus dark text on the bright selected control. Lowering the faint ink to the raised-panel color gives 1:1 on that panel and fails the test.

The minimum ratios across those placements were 7.01 to 1 for body text, 5.85 to 1 for secondary labels, 4.61 to 1 for faint labels and 4.78 to 1 for inactive labels. Each minimum occurred at a grid intersection on the hover face. Selected text measured 9.96 to 1.

## Native screens

| Native screen | Captured window |
| --- | --- |
| Settings | [General settings](screens/blueprint/settings.webp) |
| Threshing Floor | [System map and pane pool](screens/blueprint/floor.webp) |
| Help | [Keyboard reference](screens/blueprint/help.webp) |
| Workbench | [Tiled decision](screens/blueprint/blueprint-600-decision.webp) · [Wide decision](screens/blueprint/blueprint-1600-decision.webp) · [Changeset](screens/blueprint/blueprint-600-changeset.webp) · [Table](screens/blueprint/blueprint-600-table.webp) |

The skin picker keeps the saved identifier `blueprint`. Its caption and description are translated into English, Spanish, German, French, Russian, Chinese, Japanese, Korean and Hindi.

| Workbench card | 600 px | 1,600 px |
| --- | --- | --- |
| response | [View](screens/blueprint/blueprint-600-response.webp) | [View](screens/blueprint/blueprint-1600-response.webp) |
| decision | [View](screens/blueprint/blueprint-600-decision.webp) | [View](screens/blueprint/blueprint-1600-decision.webp) |
| changeset | [View](screens/blueprint/blueprint-600-changeset.webp) | [View](screens/blueprint/blueprint-1600-changeset.webp) |
| architecture | [View](screens/blueprint/blueprint-600-architecture.webp) | [View](screens/blueprint/blueprint-1600-architecture.webp) |
| artifact | [View](screens/blueprint/blueprint-600-artifact.webp) | [View](screens/blueprint/blueprint-1600-artifact.webp) |
| table | [View](screens/blueprint/blueprint-600-table.webp) | [View](screens/blueprint/blueprint-1600-table.webp) |
| markdown | [View](screens/blueprint/blueprint-600-markdown.webp) | [View](screens/blueprint/blueprint-1600-markdown.webp) |
| unclassified | [View](screens/blueprint/blueprint-600-unclassified.webp) | [View](screens/blueprint/blueprint-1600-unclassified.webp) |
