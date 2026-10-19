# Plan: documentation redesign

**Difficulty 4/10** — one public surface with a strong visual opinion; cheap to unwind before implementation.

## What it is for

The current documentation uses Terminal Delight's green palette and CRT switch, but its structure is still a conventional documentation shell: top bar, left navigation, one reading column and a grid of page links. Parker's screenshots show the product at its best when panes, themes, work, notes and agents are all visible at once. The redesign has to carry that density and motion into the docs while keeping the article easy to read.

## The shape

The workshop draws the same home page five ways: Fleet, Field Manual, Tube, Bench and Wall. Each keeps the existing documentation map and uses the real product vocabulary. The implementation will live in the documentation site, not the Rust application; this repository holds the decision and visual source so the application and its public explanation stay aligned.

The recommended shell is Fleet: the full-window composition from the screenshots becomes the home page, while articles use the Bench's split between explanation and evidence. The Tube's CRT treatment stays available for hero moments and demonstrations rather than covering every paragraph.

## Least confident decisions

1. Whether the left-hand project tree belongs on every documentation page or only on the home page.
2. Whether one supplied full-window photograph should lead the home page, or whether the drawn shell should carry the first screen.
3. How much live product chrome survives at phone width before it stops helping navigation.

## Slices

1. Tracer bullet: build the approved home shell and one real article with the existing content and URLs.
2. Reading system: settle the article anatomy, code blocks, figures, register switch and mobile navigation.
3. Rollout: migrate the live pages, test search and links, then compare the deployed result with the supplied screenshots at desktop and phone widths.
