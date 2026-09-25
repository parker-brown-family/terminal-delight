"""Write claim-check's trap anchors: matched pairs, written 2026-09-25 before any call.

Procedure (jev-decision-shape §0): parts of an input = the subject, the claim's
value, the quote. Guide words applied to each: no, more, less, as well as, part
of, reverse, other than. Kept cells, each as a pair whose second item changes one
thing that flips the right handling:

  value:more      the value adds a feature the quote lacks       -> states_part
  value:reverse   the value negates the quote                    -> contradicts
  value:less      the value names a subset of the quote          -> states (control)
  quote:no        the quote is unrelated to the field            -> silent
  quote:as-well-as the quote carries the fact among other facts  -> states (control)
  quote:other-than the quote is about another project            -> other_subject high
  value:number    the value's count differs from the quote's     -> code routes to a person
  value:hedge     a stability warning read as a guarantee        -> contradicts

Dropped: subject:no (the builder always supplies one), quote:part-of (a truncated
quote is 'silent' or 'states_part' by the same rule as value:more), value:as-well-as
(identical to value:more).

These quotes are fixtures written for the traps, not facts about the projects.
"""
import json
from pathlib import Path

HERE = Path(__file__).parent / "anchors" / "traps"
HERE.mkdir(parents=True, exist_ok=True)
WRITTEN = "2026-09-25, before any call"


def item(i, subject, field, value, quote, measured=False):
    return {"id": f"trap/{i}", "subject": subject, "field": field.split(":")[0], "field_means": field,
            "value": value, "quote": quote, "source": "fixture", "measured_by_command": measured,
            "labelled_inferred": False}


pairs = {
    "more": (
        item("more-base", "vt100 (doy) — the crate under tui-term", "sixel: whether the core decodes Sixel images",
             "yes — Sixel via the sixel-image crate",
             "Sixel graphics are supported: DCS q sequences are decoded with the sixel-image crate."),
        {"support": ["states"]},
        item("more-flip", "vt100 (doy) — the crate under tui-term", "sixel: whether the core decodes Sixel images",
             "Sixel and the Kitty graphics protocol, both via the sixel-image crate",
             "Sixel graphics are supported: DCS q sequences are decoded with the sixel-image crate."),
        {"support": ["states_part"]},
    ),
    "reverse": (
        item("reverse-base", "wterm's Ghostty core", "kitty_graphics: which parts of the Kitty graphics protocol it implements",
             "virtual placements (Unicode placeholders) are not supported — direct placements only",
             "Sixel, iTerm2/OSC 1337, animation, virtual Unicode placements, and persistence are outside the supported boundary."),
        {"support": ["states", "states_part"]},
        item("reverse-flip", "wterm's Ghostty core", "kitty_graphics: which parts of the Kitty graphics protocol it implements",
             "supports virtual placements with Unicode placeholders",
             "Sixel, iTerm2/OSC 1337, animation, virtual Unicode placements, and persistence are outside the supported boundary."),
        {"support": ["contradicts"]},
    ),
    "less": (
        item("less-base", "rio-vt — Rio's embeddable core", "sixel: whether the core decodes Sixel images",
             "yes",
             "rio-vt carries sixel, kitty and iTerm2 image protocols."),
        {"support": ["states"]},
        item("less-flip", "rio-vt — Rio's embeddable core", "sixel: whether the core decodes Sixel images",
             "no",
             "rio-vt carries sixel, kitty and iTerm2 image protocols."),
        {"support": ["contradicts"]},
    ),
    "no": (
        item("no-base", "rio-vt — Rio's embeddable core", "license: the SPDX licence identifier",
             "MIT",
             "[package]\nname = \"rio-vt\"\nlicense = \"MIT\""),
        {"support": ["states"]},
        item("no-flip", "rio-vt — Rio's embeddable core", "license: the SPDX licence identifier",
             "MIT",
             "pub fn visible_rows(&self) -> Vec<Row<Square>>"),
        {"support": ["silent"]},
    ),
    "as-well-as": (
        item("aswell-base", "libghostty-vt — Ghostty's core as a library", "build_requirements: toolchains beyond stable cargo",
             "a Zig toolchain",
             "Fetches and builds libghostty-vt.a from ghostty sources via Zig by default. Set GHOSTTY_SOURCE_DIR to use a local checkout, GHOSTTY_ZIG_SYSTEM_DIR for pre-fetched Zig packages, and LIBGHOSTTY_VT_SYS_OPTIMIZE to control optimisation."),
        {"support": ["states"]},
        item("aswell-flip", "libghostty-vt — Ghostty's core as a library", "build_requirements: toolchains beyond stable cargo",
             "cargo only; no other toolchain",
             "Fetches and builds libghostty-vt.a from ghostty sources via Zig by default. Set GHOSTTY_SOURCE_DIR to use a local checkout, GHOSTTY_ZIG_SYSTEM_DIR for pre-fetched Zig packages, and LIBGHOSTTY_VT_SYS_OPTIMIZE to control optimisation."),
        {"support": ["contradicts"]},
    ),
    "other-than": (
        item("other-base", "rio-vt — Rio's embeddable core", "kitty_graphics: which parts of the Kitty graphics protocol it implements",
             "implements the Kitty graphics protocol, including Unicode placeholders",
             "rio-vt implements the Kitty graphics protocol, including virtual placements rendered through Unicode placeholders."),
        {"support": ["states"], "other_subject": ["same", "unclear"]},
        item("other-flip", "rio-vt — Rio's embeddable core", "kitty_graphics: which parts of the Kitty graphics protocol it implements",
             "implements the Kitty graphics protocol, including Unicode placeholders",
             "Ghostty implements the Kitty graphics protocol, including virtual placements rendered through Unicode placeholders."),
        {"other_subject": ["other"]},
    ),
    "hedge": (
        item("hedge-base", "libghostty-vt — Ghostty's core as a library", "api_stability: the project's own words about API stability",
             "unstable: in development, breaking changes expected",
             "This library is currently in development and the API is not yet stable. Breaking changes are expected in future versions. Use with caution in production code."),
        {"support": ["states"]},
        item("hedge-flip", "libghostty-vt — Ghostty's core as a library", "api_stability: the project's own words about API stability",
             "stable, semver-guaranteed API suitable for production",
             "This library is currently in development and the API is not yet stable. Breaking changes are expected in future versions. Use with caution in production code."),
        {"support": ["contradicts"]},
    ),
    "number": (
        item("number-base", "rio-vt — Rio's embeddable core", "releases_last_12mo: releases in the 12 months before 2026-09-25",
             "26 releases",
             "total versions: 26 (0.0.1 on 2026-07-26 … 0.5.28 on 2026-09-17)", measured=True),
        {"support": ["states"]},
        item("number-flip", "rio-vt — Rio's embeddable core", "releases_last_12mo: releases in the 12 months before 2026-09-25",
             "31 releases",
             "total versions: 26 (0.0.1 on 2026-07-26 … 0.5.28 on 2026-09-17)", measured=True),
        {"support": ["contradicts", "states_part", "states", "silent"]},
    ),
}

# version 3: a quote that is the premise, against one that is unrelated and one that refutes
pairs["implies"] = (
    item("implies-base", "Core X (a fixture)", "sixel: whether the core decodes Sixel images", "no",
         "fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {\n    debug!(\"[unhandled hook] params={:?}, action: {:?}\", params, action);\n}"),
    {"support": ["implies", "states"]},
    item("implies-flip", "Core X (a fixture)", "sixel: whether the core decodes Sixel images", "no",
         "fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {\n    if action == 'q' { self.sixel = Some(SixelParser::new(params)); }\n}"),
    {"support": ["contradicts"]},
)
pairs["implies-build"] = (
    item("build-base", "Core Y (a fixture)", "build_requirements: toolchains beyond stable cargo", "cargo only",
         "[dependencies]\nitoa = \"1.0\"\nunicode-width = \"0.2\"\nvte = \"0.15\"\n# no build script, no build-dependencies"),
    {"support": ["implies", "states"]},
    item("build-flip", "Core Y (a fixture)", "build_requirements: toolchains beyond stable cargo", "cargo only",
         "[package]\nbuild = \"build.rs\"\n\n[build-dependencies]\ncc = \"1.0\"   # compiles vendor/libvterm/*.c"),
    {"support": ["contradicts"]},
)
pairs["unrelated"] = (
    item("unrelated-base", "Core Z (a fixture)", "sixel: whether the core decodes Sixel images", "no",
         "fn print(&mut self, c: char) { self.grid.put(self.cursor, c); }"),
    {"support": ["silent"]},
    item("unrelated-flip", "Core Z (a fixture)", "sixel: whether the core decodes Sixel images", "no",
         "Sixel graphics are not supported: DCS sequences other than DECRQSS are ignored."),
    {"support": ["states"]},
)

n = 0
for name, (base, base_expect, flip, flip_expect) in pairs.items():
    for it, exp in ((base, base_expect), (flip, flip_expect)):
        (HERE / f"{it['id'].split('/')[1]}.json").write_text(json.dumps(
            {"call": "check", "item": it, "expect": exp, "written": WRITTEN, "pair": name}, indent=2))
        n += 1
print(f"wrote {n} trap anchors to {HERE}")
