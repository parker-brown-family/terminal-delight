# Terminal Delight — Multi-Language Go-To-Market Strategy

**Date:** 2026-06-20 · **Companion to:** [the launch-plan review](./2026-06-20-competitive-feature-mining-and-launch-plan-review.md)

## Thesis

TD ships **English, Spanish, German, Chinese (中文)** today (`app/src/lang.rs`, a compile-checked `Strings` catalog + 🌐 in-app picker + CJK/Devanagari font fallback). Adding a language = one new `lang.rs` enum variant + translated strings.

> **The play:** big dev tools localize to the usual handful. Almost nobody ships a *terminal* that greets developers in Swahili, Bahasa, or Hindi. Strategically supporting **underserved languages is a cheap, high-goodwill exposure move** — large, fast-growing dev communities that incumbents ignore return outsized loyalty per string translated. A warm post on X *in their language* ("we built this for you and your devs running agentic workflows") earns reach and trust that no English launch tweet can.

The constraint isn't engineering (most are LTR Latin = zero font work); it's **getting the translations right** (native review is non-negotiable — see Risks).

## Language prioritization

| Language | Dev community | Underserved goodwill upside | X reach | TD technical caveat | Tier |
|---|---|---|---|---|---|
| **Indonesian / Bahasa** | ~2.4M GitHub devs, +7.3% YoY | **Very high** — no major IDE/terminal localizes | Indonesia = 4th-largest X market (~27M) | LTR Latin — zero font work | **1** |
| **Swahili** | East Africa belt (Nigeria #2 fastest GitHub +45.6%, Kenya accelerating) | **Extremely high** — pure blue ocean; no dev tool localizes | Emerging, loyal #AfricanDevs | LTR Latin — zero font work | **1** |
| **Hindi** | India 13.3M GitHub devs → 57.5M by 2030 | **High** — Hindi-first devs underserved; no terminal does it | ~25M X users in India | **Devanagari fallback already in TD** — verify rendering | **1** |
| **Vietnamese** | ~29M internet users, mobile-first dev base | **High** — VS Code has no official VN pack; Warp/iTerm2 = none | Growing tech X | LTR + diacritics (standard fonts) | **1** |
| **Brazilian Portuguese** | 5M+ GitHub devs, +11.6% YoY | Moderate — VS Code covers it, but no terminal does | ~23M X users (Brazil) | LTR Latin — zero work | **1** |
| **Japanese** | Tripled on GitHub 2019–24 | Moderate — covered, but strong "we see you" signal | Largest non-EN X market | CJK font already in TD | **1** |
| **French (+ Francophone Africa)** | France top-10; Senegal/DRC/Côte d'Ivoire emerging fast | Low for FR, **high** for Francophone Africa (same locale, new audience) | Strong #AfriTech | LTR Latin — zero work | **2** |
| **Bengali** | Bangladesh = **#1 fastest-growing GitHub country (+66.5% YoY)** | **Extremely high** — zero Western dev-tool localization | Growing, smaller X base | Needs Bengali (Bangla) font fallback (not yet in TD) | **2** |
| **Tagalog / Filipino** | Philippines jumped to GitHub #18 globally | **High** — zero tooling localization | Among highest global X time-on-site | LTR Latin — zero work | **2** |
| **Ukrainian** | Top-5 on SO survey; motivated loyalty | High — solidarity signal; no VS Code UA pack | Strong EE tech X | LTR Cyrillic (system fonts likely cover) | **2** |
| **Korean / Turkish** | Sizable, partially served | Low-moderate | Active tech X | CJK (KO) / LTR (TR) — minimal | **2** |
| **Arabic / Farsi** | Large MENA / Iran dev base, fully ignored by Western tools | High goodwill — **but RTL = major layout work** | Large X presence | **RTL bidi layout — separate, larger effort; do NOT bundle** | **3** |
| **Thai** | Growing SEA dev base | High — no major tool localizes | Active X | Thai script + no inter-word spaces (rendering impact) | **3** |

*Sources: GitHub Octoverse 2024; Stack Overflow Developer Survey 2024; GitHub Innovation Graph; Rest of World (Bangladesh/Nigeria/Pakistan growth); VS Code locale docs.*

## Top strategic goodwill picks (best exposure-to-effort)

1. **Indonesian / Bahasa** — 27M X users, fast-growing dev base, LTR (zero font work), and *no terminal has ever greeted an Indonesian dev in their language.* One post on Indonesian tech Twitter gets shared for weeks.
2. **Swahili** — East Africa is the fastest-growing GitHub region; LTR, costs nothing. A single Swahili tweet from a Western dev tool is a *first-ever.* Unmatchable loyalty per string.
3. **Hindi** — 13.3M devs today; **Devanagari fallback already ships in TD.** No terminal has shipped a Hindi UI. A differentiated "we built this for you" signal before anyone else thinks of it.
4. **Vietnamese** — a rare Tier-1 underserved community that's *also* technically trivial (LTR + standard diacritics).

## Sample localized launch posts

> ⚠ **All require native-speaker (developer) review before shipping** — a bad translation here is worse than English-only (see Risks). English gloss under each.

**Indonesian**
> Terminal Delight sekarang tersedia dalam Bahasa Indonesia 🖥️🇮🇩
> Kami tahu komunitas developer Indonesia berkembang pesat — dan kalian layak punya tools yang bicara bahasa kalian. Ini bukan sekadar terjemahan. Kami membangun ini untuk kalian. GPU-native, open source (MIT), local-first. Untuk yang memantau agen AI semalaman — kami paham. 🌐

*"…now in Bahasa Indonesia. We know the Indonesian dev community is growing fast — and you deserve tools that speak your language. This isn't just a translation. We built this for you… For those watching AI agents overnight — we understand."*

**Swahili**
> Terminal Delight iko katika Kiswahili sasa hivi 🖥️🌍
> Jumuiya ya waendelezaji wa Afrika Mashariki inakua kwa kasi — na mnastahili zana zinazozungumza lugha yenu. Tulijenga hii kwa ajili yenu. Chanzo huria (MIT), ya ndani kwanza, ya GPU. Kwa wale wanaofuatilia mawakala wa AI usiku — tunajua hilo. 🤝

*"…now in Swahili. The East African dev community is growing fast — and you deserve tools that speak your language. We built this for you. Open source (MIT), local-first, GPU-native…"* ⚠ Kenya/Tanzania register differs — validate with a regional native speaker.

**Hindi**
> Terminal Delight अब हिंदी में उपलब्ध है 🖥️🇮🇳
> भारत के डेवलपर समुदाय के लिए — जो रात भर AI agents को देखते हैं, जो नए tools बनाते हैं, जो दुनिया को बदल रहे हैं। यह हमने आपके लिए बनाया है। GPU-native • MIT • local-first • हिंदी UI 🌐

*"…now in Hindi. For India's developer community — those who watch AI agents overnight, build new tools, change the world. We built this for you."* ⚠ Verify Devanagari rendering in the actual TD UI before announcing.

## Rollout plan

**Tier 1 — ship with / right after launch (LTR, low/zero font effort):** Indonesian, Swahili, Hindi (verify Devanagari), Vietnamese, Brazilian Portuguese, Japanese. *Each = strings only (Hindi/JP fonts already covered).*

**Tier 2 — next sprint:** French (Francophone-Africa angle), Bengali (**+Bangla font fallback**), Tagalog, Ukrainian (verify Cyrillic), Korean, Turkish.

**Tier 3 — separate tracked effort:** **Arabic & Farsi need bidirectional (RTL) layout — real engineering, not just strings.** Scope it as its own roadmap item; *a broken RTL UI is worse than English-only.* Thai needs a font + no-space-segmentation check first.

## Risks

- **MT quality.** Machine-translated UI strings fail in *embarrassing* (not neutral) ways. A bad Swahili "Inherit theme from parent pane" signals nobody who speaks Swahili was involved. **Policy: every language gets a native-speaker-developer review before the picker exposes it.**
- **Bad localization > no localization.** English-only reads as a limitation; a tone-deaf local translation reads as disrespect. The entire goodwill upside depends on the translations being *good.*
- **Maintenance debt.** Every new English string is a gap in 9 other languages until translated. Policy: untranslated strings **fall back to English** (never broken strings); track translation debt as first-class issues.
- **Reach expectations.** This is brand/loyalty investment, not week-1 DAU. Measure share-of-voice + sentiment, not installs.
- **Translator sourcing** is the real constraint for underserved languages — recruit native reviewers *before* announcing, not after.
