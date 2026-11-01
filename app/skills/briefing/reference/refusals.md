# Refusals

`brief.py` stops rather than guess. Each refusal prints one line starting `brief:` and exits 1, and nothing on disk has changed. `resolve` exiting 2 is not a refusal: it means some comments need review, and every comment is still in the output.

## Starting: `new`

| Message begins | Meaning | Do this |
|---|---|---|
| `… exists; new never overwrites a file` | The source path is taken | Pick a new path, or edit the existing source |
| `name the source .md for Markdown or .src.html for HTML` | The suffix chose no medium | Rename the target |
| `use a stable lowercase brief id, such as …` | The id must match `[a-z][a-z0-9-]*` | Use a short kebab-case name that will outlive the title |
| `name the purpose: …` | The purpose was blank | Give `summary`, `engineering`, `decision`, or a stated outcome |

## Building: `html`

| Message begins | Meaning | Do this |
|---|---|---|
| `an HTML build reads the decision-brief and fancy-pants-brief skills …` | The sibling skills that hold the shared look and notes engine are not installed beside this one | Install the three directories side by side, or write the brief in Markdown. `brief.py doctor` confirms |
| `no wallpaper: brief-wall: …` | The wallpaper picker failed; its own reason follows | Fix the named cause, or pass `--no-wall` for the stylesheet's own dark ground |
| `… was edited after its last build and … does not have those edits` | Someone edited the built page; a rebuild would drop that edit | Port the edit into the source and rebuild. Only if the edit should go, pass `--discard-output-edits`; the edited page is kept as `.bak`. If nobody touched the page, the helper's fingerprint rules may have changed since it was built: compare it with a fresh build, then discard |
| `… exists and was not built by this helper` | The output path holds some other file | Choose another output path, or move that file aside |
| `… belongs to the brief '…', not '…'` | The existing page has another id | Build with the id the message names, or choose a new output path |
| `--id … disagrees with the source, which says …` / `--purpose …` | A flag contradicts the source's `<!-- briefing: -->` comment | Drop the flag, or change the comment |
| `name the brief: pass --id and --purpose …` | The source does not name itself and no flags were given | Pass both, or add the comment `new` writes |
| `a source carries at most one <!-- briefing: {...} --> identity object` | Two identity comments, or one that is not an object | Keep one |
| `output changed during build; retry from the latest file` | Something wrote the page while it was being rebuilt, usually a reader saving notes | Rebuild again; the reader's save is intact |
| `keep <!--BRIEF:head--> and <!--BRIEF:notes--> outside the <!--brief-content--> region` | A slot sits inside the fingerprinted content | Move `BRIEF:head` into `<head>` and `BRIEF:notes` to the end of `<body>`, after `<!--/brief-content-->` |
| `expected one BRIEF:… placeholder` / `unknown or out-of-order BRIEF placeholder` | The source needs exactly `<!--BRIEF:head-->` then `<!--BRIEF:notes-->` | Start from `brief.py new` and compare |
| `expected one brief-content region …` | The source needs one `<!--brief-content-->` … `<!--/brief-content-->` pair | Wrap the header and main in it |
| `source needs unique ids and explicit anchors on every h2 and figure` | A duplicate id, or an `h2` or `figure` without `data-brief-anchor` | Give each one `id="x" data-brief-anchor="x"` |
| `a data-brief-anchor must equal its explicit, lowercase id` | Anchor and id differ, or the anchor is not kebab-case | Make them the same lowercase name |
| `HTML output needs an .html suffix` / `source and output must be different files` | The output path is wrong | Name the output `name.html` beside `name.src.html` |
| `report-notes: …` / `report-concurs: …` / `…: malformed JSON` / `…: expected exactly one island` / `…: unsupported annotation format` | The existing page's saved notes are damaged or come from a newer format | Do not rebuild over them. Open the page, copy its map, and repair or report the file. Rebuilding would lose the reader's notes |
| `backup collision` | A `.bak` with the same name holds different bytes | Move the stray `.bak` aside and rebuild |
| `canonical notes anchor hook changed …` / `… contains a closing script tag` | The shared notes engine changed upstream | Review the adapter in `build` and its test together; do not edit `decision-brief` to make this pass |
| `the assembled page fingerprints differently from its source …` | An internal guarantee failed | Report it with the source attached |
| `every decision a reader can stamp (.ask, .grill-q, or .q inside a .grill) needs id="x" data-brief-anchor="x" …` | A decision without an anchor takes no note and no CONCUR; the message lists them by title | Give each one `id="q-short-name" data-brief-anchor="q-short-name"` |
| `derives_from must be a list …` / `calls must name a .calls.json ledger …` | The identity comment's new fields are malformed | Use `new --derives-from` and `new --calls` to see their shape |

## Checking: `check`

| Message begins | Meaning | Do this |
|---|---|---|
| `unwritten scaffold passage: …` | A `TODO(brief)` from `new` remains | Write that passage. To name the marker itself in a brief, write `TODO&#40;brief)` in HTML or `TODO\(brief)` in Markdown; both render as the marker and pass |
| `broken local link: …` | A relative link, image or `file://` URL points nowhere. Every broken link is listed in one message, separated by `;` | Fix the paths, or create the evidence notes |
| `broken local fragment: …` | The file exists but has no such heading, id or `^block` | Fix the fragment; a renamed heading changes its slug, an explicit id does not |
| `cannot verify local fragment: …` | A fragment into a file that is neither Markdown nor HTML | Link to the file without a fragment, and name the place in prose |
| `every h2 and figure needs an explicit data-brief-anchor` | A built page lost an anchor | Fix the source and rebuild |
| `duplicate anchors: …` | Two targets share a name | Rename one before anyone comments on it |
| `Markdown needs one <!-- briefing: {...} --> identity comment` / `brief identity …` / `brief metadata needs an explicit purpose` | The document does not say which brief it is | Add the identity comment; `new` shows its shape |
| `supported now: .md and .html; MP4 is reserved` | Another file type | Video is not built yet |
| `every decision a reader can stamp …` | A built page has a decision without an anchor; see the same row under `html` | Fix the source and rebuild |
| `derives_from names …, which is not there` | A parent brief moved or was deleted | Fix the path, or drop it from the identity comment |
| `open calls missing from the brief: …` | The brief names a calls ledger and leaves an open or asked call out | Paste the section `calls LEDGER --html` prints into the source and rebuild |
| `call rows rendered from other words or evidence than the ledger holds: …` | A row was rendered from another ledger or an earlier version of the call, or edited after it was printed | Paste a fresh `calls LEDGER --html` and rebuild; rows are generated, never edited |
| `call rows that are not one call of the ledger, or sit inside another row: …` | A row names a call the ledger does not hold, or rows are nested | Remove it, or paste a fresh section |
| `the brief shows call rows but names no calls ledger: …` | Rows with no ledger to answer them | Add `"calls": "NAME.calls.json"` to the identity comment, or start with `new --calls` |

## While the work runs: `orient`, `call`, `calls`, `verdict`

`call` exiting 3 is not a refusal: the call is recorded as `asked`, and the agent stops to ask. `calls` exiting 2 means the work has not settled; every call is still listed.

| Message begins | Meaning | Do this |
|---|---|---|
| `… a calls ledger is named NAME.calls.json …` | The ledger path has another name | Name it `something.calls.json`; no other JSON is ever written over |
| `… no calls ledger here …` | `calls`, `verdict` or `orient --calls` named a ledger that does not exist | `call` starts one; check the path |
| `… malformed JSON …` / `… not a format-1 briefing calls ledger` / `every call needs an id and a known status, and …` | The ledger is damaged; the message names the call and the missing or malformed field. A field that is absent is refused, never read as empty | Repair it by hand; nothing was written |
| `… appears twice …` | Two calls share an id | Rename or remove one by hand |
| `… with no verdict recorded: who answered, and when` / `… is overturned with no reason recorded` / `… carries a verdict` | A status and its verdict disagree | Record answers with `verdict` or `calls --sync`, never by editing the status |
| `…feedback.json: expected a format-1 envelope …` / `…: malformed JSON` | `orient` read a Markdown brief's comment file that is damaged | Regenerate it with `feedback`, then restore the reader's words |
| `… is missing; … was decided on it …` | A call's evidence file is gone | Restore it, or supersede the call |
| `a call needs --title …` / `--chose` / `--because` | A call must say what was decided, what was chosen and why | Write them in plain words |
| `--uncertainty is one of …` / `--posture is one of …` / `--door is …` | An unknown value | Use one listed; a reading you cannot make is `unknown` |
| `… is already in the ledger …` | The id is taken | Pass `--id call-new-name`, or supersede the old call |
| `evidence … is not a file …` | The screenshot or file is missing | Save it first; a call records what it was decided on |
| `… changed since … was recorded …` | The evidence changed before the brief was rendered | Record the call again, or supersede it |
| `… is over 3 MB …` | A screenshot too large to embed | Save a smaller one and record the call again |
| `an overturn carries the person's reason in --note …` | An overturn without its reason | Quote the person's reason |
| `… is not in … ledger …` | The call id is unknown | `calls LEDGER` lists the ids |
| `sync reads the CONCUR stamps saved in a built HTML brief` | `--sync` was given Markdown | Record Markdown answers with `verdict` |
| `--derives-from names briefs that are not there` / `--calls names a NAME.calls.json ledger` | `new` was given a bad parent or ledger path | Fix the path |

`check` also reports facts that are not failures: `edited_since_build` on a built page, and `external_links_unchecked`, the number of web links it did not fetch.

## Returning comments: `feedback` and `resolve`

| Message begins | Meaning | Do this |
|---|---|---|
| `the map is neither a format-1 JSON envelope nor Copy map text …` | The text has no `NOTES —` line, even after removing a chat quote, and is not JSON | Ask the reader to press copy map again and paste all of it, or read their message as ordinary feedback |
| `unsupported feedback format` | The envelope's `format` is not 1 | Read it by hand against the document |
| `feedback needs document and comments` / `each comment needs a target and text` | The envelope is incomplete | Restore the missing fields from the reader's original |
| `empty comment without an explicit stance` | An empty comment says nothing; it is not agreement | Remove it, or set `stance` to `agree` or `disagree` if the reader stated one |
| `invalid stance; unknown is null` | `stance` must be `agree`, `disagree` or `null` | Use `null` when the reader stated none |
