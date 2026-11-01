# HTML

Before drawing, read `decision-brief/reference/pictures.md` and `decision-brief/reference/glass.md` from the sibling skill, if it is installed: the first is how to draw a figure that argues, the second is what the glass look expects of a page. Reuse `fancy-pants-brief` for the house look: dark glass over one random Omarchy wallpaper, themed ink, gel controls and a section rail. The new brief keeps its chosen purpose; using the assets does not turn every brief into a decision request.

Start with `scripts/brief.py new name.src.html --id STABLE-ID --purpose PURPOSE`, which writes a source with every marker in place, one section, one drawn figure and `TODO(brief)` where the writing goes. It also writes `<!-- briefing: {"id": …, "purpose": …} -->` into the head, so `html name.src.html name.html` needs no flags; a flag that disagrees with that comment is refused. [The engineering source](../examples/engineering.src.html) is a finished example. A complete source has one `<!--BRIEF:head-->` slot in its head, one `<!--BRIEF:notes-->` slot at the end of its body, and one `<!--brief-content-->` / `<!--/brief-content-->` pair around the authored header and main. The assembler rejects unknown or repeated slots, and slots inside the content region: anything assembled there would change the page's fingerprint and make every comment look stale from the start.

Give each significant comment target an explicit stable id and matching `data-brief-anchor`. Every section heading and figure needs one, and so does every decision a reader can stamp: an `.ask`, a `.grill-q`, or a `.q` inside a `.grill`. Name decisions `q-…` and calls `call-…`, so `orient` can tell them apart at a glance. Use the same id after renaming the heading; never reuse a removed id for a different claim.

```html
<h2 id="sec-return" data-brief-anchor="sec-return">Returning a comment</h2>
<figure id="fig-target" data-brief-anchor="fig-target">
  <span class="lbl">01 · A comment's target</span>
  <!-- Draw the mechanism here. -->
  <figcaption>A removed target keeps its comment unresolved.</figcaption>
</figure>
```

Build with `scripts/brief.py html SOURCE OUTPUT`, adding `--id` and `--purpose` when the source does not name itself. It reads the existing canonical notes assets, with a checked one-line adaptation for explicit anchors. There is no forked asset or storage-format migration. Rebuild through the same command; do not paste empty notes islands over a reader's comments.

The output is self-contained for its rendering and annotation UI. Evidence links are deliberate navigation to adjacent files; share their directory when those sources must travel with it. Drawing assets and scripts are inline, with no CDN or network dependency. A linked evidence note is not a rendering dependency.

The new output's handoff bridge shows an unresolved-feedback notice when carried comments have an unknown revision or missing target. Copy Map includes those removed targets and their reader text. The older skill's map generator and storage stay unchanged.

Edit the source, never the built page. A rebuild replaces the page's content with the source's, so `html` refuses when the page's content no longer matches the revision it was built with, and `check` reports that as `edited_since_build: true`. Port the edit into the source and rebuild. `--discard-output-edits` rebuilds anyway, keeps the edited page as a `.bak`, and marks any carried comments' revision unknown, since the reader commented on wording that is now gone. `html` also refuses to write over a file it did not build.

The wallpaper comes from `decision-brief/scripts/brief-wall`, which needs Omarchy themes and `vips`. Where those are missing, the refusal carries `brief-wall`'s own reason. `--no-wall` builds on the stylesheet's own dark ground instead. That page is complete and commentable but outside the Omarchy house look; use it where the house look is not available, not to skip it.

On rebuilding an existing output, the tool preserves its wallpaper, both notes islands including revisions, and the last reader-notes mirror byte for byte. It validates the old islands before replacement and keeps the old bytes in a content-addressed `.bak` file. Detected concurrent writes cause refusal; editing and rebuilding simultaneously still needs host-level coordination. Never claim an atomic replace is a full multi-writer transaction.

If a canonical source hook changes, assembly stops for review. Do not silently remove the guard or mutate the older skill to make this one pass. Review and update the small adaptation alongside its tests.

Verify with the sibling glass gate, this skill's local-link checker and an isolated headless browser. Check at a narrow pane width and a phone width. A figure's text must fit its own viewBox, not merely avoid page overflow. Every important target needs an actual note control; a total count cannot establish coverage.

In Terminal Delight, the native reader can submit comments into the original HTML. A standalone browser stores live edits in localStorage and exports a map. Test the host's persistence path before reporting a disk save. The existing notes engine can restore a browser-held comment deleted by another writer; this skill does not claim to solve that conflict.
