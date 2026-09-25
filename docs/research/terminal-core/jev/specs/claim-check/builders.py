"""Frame builder and code axes for the claim-check spec.

An item is one fact a research agent recorded about a candidate terminal core,
with the verbatim passage the agent quoted as its support (the round's quote
rule). Items are built from the research-delight run by
docs/research/terminal-core/scripts/claims.py:

    {"id": "rio-vt/license", "subject": "rio-vt — Rio's embeddable core",
     "field": "license", "field_means": "the SPDX licence identifier ...",
     "value": "MIT", "quote": "MIT", "source": "measured 2026-09-25: gh api ...",
     "measured_by_command": true, "labelled_inferred": false}

Everything here is code-owned: whether a quote exists, whether the numbers in the
value appear in it, whether the source is the project's own. Jev is asked only
whether the passage states the fact.
"""

from __future__ import annotations

import re
from urllib.parse import urlparse

# Hosts that are a project's own primary sources for the candidates in this run.
PRIMARY_HOSTS = (
    "github.com", "raw.githubusercontent.com", "crates.io", "docs.rs", "lib.rs",
    "gitlab.com", "codeberg.org", "launchpad.net", "sw.kovidgoyal.net", "ghostty.org",
    "libghostty.tip.ghostty.org", "rioterm.com", "wezterm.org", "wezfurlong.org",
    "mitchellh.com", "neovim.io", "contour-terminal.org", "wterm.dev", "zellij.dev",
    "invisible-island.net", "vt100.net",
)

NUM = re.compile(r"(?<![\w.])(\d[\d,]*(?:\.\d+)?)(?![\w.])")


HEADLINE_BREAK = re.compile(r"\s[—–]\s|\s-\s|;\s|:\s|\.\s")


def headline(value) -> str:
    """The value's first assertion: the text before its first dash, semicolon, colon or
    sentence end. Version 1 checked whole values and found them composite by design (the
    rubric asked agents to add context), so 87% came back 'states part' — correctly.
    Version 2 checks the headline and labels the rest as the agent's elaboration."""
    text = " ".join(str(value).split())
    pos = 1
    while True:
        m = HEADLINE_BREAK.search(text, pos)
        # 'kitty-keyboard: yes' is a label and its answer, not a headline and its gloss:
        # a colon after a single token (no space before it) is not a break (version 3).
        if m and m.group(0).startswith(":") and " " not in text[: m.start()]:
            pos = m.end()
            continue
        break
    head = text[: m.start()] if m else text
    return head.strip().rstrip(".,")[:240]


def claim_packet(item: dict, ctx: dict) -> dict | None:
    """The state one check sees: the subject, the claim's headline, and the quote. No
    source URL, no agent remark, and not the elaboration after the headline."""
    if not item.get("quote"):
        return None
    return {
        "subject": item["subject"],
        "claim": {"field": item["field_means"], "value": headline(item["value"])},
        "quote": item["quote"],
    }


def quote_present(item: dict, ctx: dict) -> bool:
    return bool((item.get("quote") or "").strip())


def _numbers(text: str) -> set[str]:
    out = set()
    for m in NUM.finditer(text or ""):
        n = m.group(1).replace(",", "")
        if len(n.replace(".", "")) >= 2:  # single digits are too common to test
            out.add(n.rstrip("0").rstrip(".") if "." in n else n)
    return out


def numbers_match(item: dict, ctx: dict) -> str:
    """Every number of two or more digits in the headline appears in the quote or the
    measuring command: 'match', 'mismatch', or 'none' when the headline has no such
    number. Version 1 returned None for 'none', which jds rightly reads as unknown, and
    that voided the verdict for 112 of 315 facts."""
    wanted = _numbers(headline(item.get("value", "")))
    if not wanted:
        return "none"
    have = _numbers(item.get("quote", "")) | _numbers(item.get("source", ""))
    return "match" if wanted <= have else "mismatch"


def primary_source(item: dict, ctx: dict) -> bool | None:
    src = (item.get("source") or "").strip()
    if not src:
        return None
    if src.startswith("measured"):
        return True
    host = urlparse(src.split()[0]).netloc.lower()
    if not host:
        return None
    return any(host == h or host.endswith("." + h) for h in PRIMARY_HOSTS)


def elaborated(item: dict, ctx: dict) -> bool:
    """The value carries more than its headline: the rest is the agent's elaboration,
    shown on the page but not checked against the quote."""
    return len(" ".join(str(item.get("value", "")).split())) > len(headline(item.get("value", ""))) + 3
