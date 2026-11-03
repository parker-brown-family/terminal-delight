#!/usr/bin/env python3
"""Start, build and check a brief, then place a reader's returned comments against it.

new → write → html (HTML only) → check → deliver → feedback / resolve. `doctor` says what works here.
While the work runs: orient re-reads what was approved; call records a decision nobody made;
calls lists them, renders them into the closing brief and says whether the work has settled; verdict
records the reader's answer."""
import argparse
import base64
from collections import Counter
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
from html import escape as html_escape
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from urllib.parse import quote, unquote, urlsplit
try:
    import fcntl  # POSIX; elsewhere the compare-before-replace in `atomic` is the only guard
except ImportError:
    fcntl = None

HERE = Path(__file__).resolve().parents[1]
KIT = HERE.parent / 'decision-brief'
LOOK = HERE.parent / 'fancy-pants-brief'
SAFE_ID = re.compile(r'^[a-z][a-z0-9-]*$')
META = re.compile(r'<!-- briefing:\s*(\{.*?\})\s*-->', re.S)
MIRROR = re.compile(r'<!--\nREADER NOTES —.*?-->', re.S)
CONTENT = re.compile(r'<!--brief-content-->(.*?)<!--/brief-content-->', re.S)
VOID_TAGS = set('area base br col embed hr img input link meta param source track wbr'.split())
# An HTML build reads these from the sibling skills; Markdown needs none of them.
KIT_FILES = [KIT / 'assets/base.css', KIT / 'assets/notes.css', KIT / 'assets/notes.js',
             KIT / 'reference/notes-markup.html', KIT / 'scripts/brief-wall', LOOK / 'assets/sections.js']
# A scaffold's unwritten passages. `check` refuses a brief that still carries one.
UNWRITTEN = 'TODO(brief)'
# What notes.js gives a CONCUR stamp: `.ask`, `.grill-q`, and `.q` inside a `.grill` but not inside an `.ask`.
DECISION_CLASSES = {'ask', 'grill-q'}
# Where the readings behind a judgement call come from. An agent fills it today; a System 1 model may later.
SLOT = '{System 1 JUDGEMENT Surface Placeholder}'
LEVELS = ('low', 'medium', 'high', 'unknown')
POSTURES = ('relaxed', 'standard', 'strict')
VERDICTS = ('concurred', 'overturned', 'superseded')
CALL_ID = re.compile(r'^call-[a-z0-9][a-z0-9-]*$')
IMAGE_TYPES = {'.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.webp': 'image/webp', '.gif': 'image/gif'}
EMBED_LIMIT = 3 * 1024 * 1024
EVIDENCE_KINDS = ('screenshot', 'file')
SHA256 = re.compile(r'^sha256:[0-9a-f]{64}$')
# A browser closes an open <p> when one of these starts inside it; the parsers below do the same.
BLOCKS = set('address article aside blockquote center dd details dialog dir div dl dt fieldset figcaption figure footer form '
             'h1 h2 h3 h4 h5 h6 header hgroup hr li listing main menu nav ol p pre search section summary table ul xmp'.split())
# The elements that bound "button scope": an open <p> above one of these is out of reach.
SCOPE_BOUNDARY = set('applet button caption html marquee object table td template th'.split())


class Refusal(ValueError):
    pass


def read(path):
    return Path(path).read_bytes().decode('utf-8')


def digest(text):
    return 'sha256:' + hashlib.sha256(text.encode()).hexdigest()


def encode(value):
    return json.dumps(value, ensure_ascii=False, indent=2).replace('</', '<\\/').replace('<!', '\\u003c!')


class Page(HTMLParser):
    def __init__(self, text):
        super().__init__()
        self.ids, self.targets, self.links, self.headings, self.figures = [], [], [], [], []
        self.feed(text)

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        if attrs.get('id'):
            self.ids.append(attrs['id'])
        if 'data-brief-anchor' in attrs:
            anchor = attrs['data-brief-anchor']
            if not anchor or attrs.get('id') != anchor or not SAFE_ID.fullmatch(anchor):
                raise Refusal('a data-brief-anchor must equal its explicit, lowercase id')
            self.targets.append(anchor)
        if tag == 'h2':
            self.headings.append(attrs.get('data-brief-anchor'))
        if tag == 'figure':
            self.figures.append(attrs.get('data-brief-anchor'))
        if tag == 'a' and attrs.get('href'):
            self.links.append(attrs['href'])
        if tag in ('img', 'source') and attrs.get('src'):
            self.links.append(attrs['src'])


class Decisions(HTMLParser):
    """The decisions a reader can stamp, in document order, with each one's anchor and title.

    Mirrors notes.js's CONCUR selector: `.ask`, `.grill-q`, and `.q` inside a `.grill` but not inside an
    `.ask`. The title is the text of the first h3, h4 or b inside it."""
    def __init__(self, text):
        super().__init__()
        self.stack, self.found, self.capture = [], [], None
        self.feed(text)

    def owner(self):
        return next((index for _, _, index, _ in reversed(self.stack) if index is not None), None)

    def handle_starttag(self, tag, attributes):
        if tag in VOID_TAGS:
            return
        if tag in BLOCKS:
            # A browser closes a <p> in button scope here, with whatever is still open inside it.
            for depth in range(len(self.stack) - 1, -1, -1):
                if self.stack[depth][0] == 'p':
                    self.handle_endtag('p')
                    break
                if self.stack[depth][0] in SCOPE_BOUNDARY:
                    break
        attrs = dict(attributes)
        classes = set((attrs.get('class') or '').split())
        in_grill = any(grill for _, grill, _, _ in self.stack)
        in_ask = any(ask for _, _, _, ask in self.stack)
        index = None
        # notes.js: '.grill .q:not(.ask .q), .grill-q, .ask' — only an .ask ancestor excludes a .q.
        if classes & DECISION_CLASSES or ('q' in classes and in_grill and not in_ask):
            self.found.append({'anchor': attrs.get('data-brief-anchor'), 'title': '', 'tag': tag})
            index = len(self.found) - 1
        self.stack.append((tag, 'grill' in classes, index, 'ask' in classes))
        owner = self.owner()
        if tag in ('h3', 'h4', 'b') and owner is not None and not self.found[owner]['title'] and self.capture is None:
            self.capture = (owner, len(self.stack) - 1)

    def handle_startendtag(self, tag, attrs):
        if tag not in VOID_TAGS:
            self.handle_starttag(tag, attrs)
            self.handle_endtag(tag)

    def handle_endtag(self, tag):
        for i in range(len(self.stack) - 1, -1, -1):
            if self.stack[i][0] == tag:
                if self.capture and self.capture[1] >= i:
                    self.capture = None
                del self.stack[i:]
                return

    def handle_data(self, data):
        if self.capture:
            self.found[self.capture[0]]['title'] += data

    def decisions(self):
        return [{**found, 'title': ' '.join(found['title'].split())} for found in self.found]


def unanchored_decisions(text):
    return [d for d in Decisions(text).decisions() if not d['anchor']]


UNANCHORED = ('every decision a reader can stamp (.ask, .grill-q, or .q inside a .grill) needs id="x" '
              'data-brief-anchor="x"; without one it takes no note and no CONCUR. Unanchored: ')


def heading_slug(heading):
    return re.sub(r'[^\w\- ]', '', re.sub(r'\s+#+\s*$', '', heading).strip().lower()).replace(' ', '-')


def markdown_prose(text):
    """Markdown with fenced code removed, so an example inside a fence is not a link or an anchor."""
    return re.sub(r'^ {0,3}(`{3,}|~{3,})[^\n]*\n.*?^ {0,3}\1\s*$', '', text, flags=re.M | re.S)


def markdown_links(prose):
    """Inline links and images, with or without a title or angle brackets, plus reference definitions."""
    prose = re.sub(r'(`+)[^`\n]+?\1', '', prose)
    inline = re.findall(r'\[[^\]\n]*\]\(\s*(<[^>\n]+>|[^\s)]+)(?:\s+(?:"[^"\n]*"|\'[^\'\n]*\'|\([^)\n]*\)))?\s*\)', prose)
    defined = re.findall(r'^ {0,3}\[(?!\^)[^\]\n]+\]:\s*(<[^>\n]+>|\S+)', prose, re.M)
    return [link[1:-1] if link.startswith('<') else link for link in inline + defined]


def anchors_in(text):
    """Every anchor a link may name in a linked Markdown or HTML file."""
    prose = markdown_prose(text)
    return Page(prose).ids + block_ids(prose) + [heading_slug(h) for h in re.findall(r'^#{1,6}\s+(.+)$', prose, re.M)]


def block_ids(prose):
    """Obsidian block ids: `^name` alone on a line, or ending a paragraph's line after a space."""
    return re.findall(r'(?:^|[ \t])\^([a-z][a-z0-9-]*)[ \t]*$', prose, re.M)


class Content(HTMLParser):
    """Hash authored meaning after browser snapshot serialization, without its note UI."""
    def __init__(self, text, keep_nav=False):
        super().__init__()
        self.parts, self.skip, self.keep_nav, self.duplicated = [], 0, keep_nav, False
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        if len({key for key, _ in attrs}) != len(attrs):
            self.duplicated = True  # a browser shows the first of two same-named attributes; this parser keeps the last
        attrs = {key: '' if value is None else value for key, value in attrs}
        if tag == 'nav' and 'sections' in attrs.get('class', '').split() and not self.keep_nav:
            self.skip += 1
        if self.skip:
            return
        for name in ('data-nid', 'data-ntitle'):
            attrs.pop(name, None)
        if 'data-depth' in attrs:
            attrs.pop('hidden', None)  # Evidence depth is a reader control.
        if tag == 'details':
            attrs.pop('open', None)
        if 'class' in attrs:
            classes = set(attrs['class'].split()) - {'notable', 'has-note', 'concurrable', 'has-concur', 'concur-jolt'}
            if classes:
                attrs['class'] = ' '.join(sorted(classes))
            else:
                del attrs['class']
        if tag != 'tbody' or attrs:
            self.parts.append(('start', tag, sorted(attrs.items())))

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        if tag not in VOID_TAGS:
            self.handle_endtag(tag)

    def handle_endtag(self, tag):
        if self.skip:
            if tag == 'nav':
                self.skip -= 1
            return
        if tag != 'tbody':
            self.parts.append(('end', tag))

    def handle_data(self, data):
        if not self.skip and data.strip():
            # A browser save turns CRLF into LF; the fingerprint must not see the difference.
            self.parts.append(('text', data.replace('\r\n', '\n').replace('\r', '\n')))


def content_revision(text, identity, purpose):
    regions = list(CONTENT.finditer(text))
    if len(regions) != 1:
        raise Refusal('expected one brief-content region around authored header and main')
    return digest(json.dumps([identity, purpose, Content(regions[0][1]).parts], ensure_ascii=False))


class Scripts(HTMLParser):
    """Locate actual script (or style) elements, ignoring tag-shaped words inside their data."""
    def __init__(self, text, tag='script'):
        super().__init__()
        self.text, self.tag, self.blocks, self.active = text, tag, [], None
        self.offsets = [0] + [match.end() for match in re.finditer('\n', text)]
        self.feed(text)

    def position(self):
        line, column = self.getpos()
        return self.offsets[line - 1] + column

    def handle_starttag(self, tag, attrs):
        if tag == self.tag:
            start = self.position()
            self.active = (dict(attrs), start, start + len(self.get_starttag_text()))

    def handle_endtag(self, tag):
        if tag == self.tag and self.active is not None:
            start = self.position()
            end = self.text.find('>', start) + 1
            self.blocks.append((*self.active, start, end))
            self.active = None


def island(text, name):
    scripts = Scripts(text)
    matches = [block for block in scripts.blocks if block[0].get('id') == name]
    if len(matches) != 1 or (scripts.active and scripts.active[0].get('id') == name):
        raise Refusal(f'{name}: expected exactly one island')
    attrs, start, content_start, content_end, end = matches[0]
    payload = text[content_start:content_end]
    if name.startswith('report-'):
        if attrs.get('data-format') != '1':
            raise Refusal(f'{name}: unsupported annotation format')
    try:
        value = json.loads(payload)
    except ValueError as exc:
        raise Refusal(f'{name}: malformed JSON') from exc
    if not isinstance(value, dict):
        raise Refusal(f'{name}: expected object')
    return text[start:end], value


def notes(text):
    nb, nm = island(text, 'report-notes')
    cb, cm = island(text, 'report-concurs')
    if not all(isinstance(v, list) and all(isinstance(n, dict) and isinstance(n.get('text'), str) for n in v) for v in nm.values()):
        raise Refusal('report-notes: expected lists of reader comments')
    if not all(isinstance(v, str) for v in cm.values()):
        raise Refusal('report-concurs: expected timestamp strings')
    return nb, nm, cb, cm


def inspect(path):
    text = read(path)
    if path.suffix == '.html':
        _, meta = island(text, 'brief-meta')
        page = Page(text)
        anchors, links = page.ids, page.links
        if meta.get('medium') != 'html':
            raise Refusal('brief-meta: medium must be html')
        if not isinstance(meta.get('revision'), str):
            raise Refusal('brief-meta: missing content revision')
        if not isinstance(meta.get('id'), str) or not isinstance(meta.get('purpose'), str):
            raise Refusal('brief-meta: missing identity or purpose')
        revision = content_revision(text, meta['id'], meta['purpose'])
    elif path.suffix == '.md':
        matches = list(META.finditer(text))
        if len(matches) != 1:
            raise Refusal('Markdown needs one <!-- briefing: {...} --> identity comment')
        meta = json.loads(matches[0][1])
        if not isinstance(meta, dict):
            raise Refusal('Markdown identity: expected object')
        if meta.get('medium') != 'markdown':
            raise Refusal('Markdown identity: medium must be markdown')
        prose = markdown_prose(text)
        page = Page(prose)
        anchors = page.ids + block_ids(prose)
        # Heading links are accepted; stable feedback targets still use explicit ids.
        explicit = anchors[:]
        for heading in re.findall(r'^#{1,6}\s+(.+)$', prose, re.M):
            slug = heading_slug(heading)
            if slug not in anchors:
                anchors.append(slug)
        links = page.links + markdown_links(prose)
        revision = digest(text)
    else:
        raise Refusal('supported now: .md and .html; MP4 is reserved')
    if not isinstance(meta.get('id'), str) or not SAFE_ID.fullmatch(meta['id']):
        raise Refusal('brief identity needs a stable lowercase id')
    if not isinstance(meta.get('purpose'), str) or not meta['purpose'].strip():
        raise Refusal('brief metadata needs an explicit purpose')
    duplicates = [k for k, v in Counter(explicit if path.suffix == '.md' else anchors).items() if v > 1]
    if duplicates:
        raise Refusal('duplicate anchors: ' + ', '.join(duplicates))
    return text, meta, revision, anchors, links


def local_target(path, link):
    """The local file a link opens, or None for a link to another machine."""
    parsed = urlsplit(link)
    if parsed.scheme == 'file' and parsed.netloc in ('', 'localhost'):
        return Path(unquote(parsed.path)), parsed.fragment
    if parsed.scheme or parsed.netloc:
        return None, None  # External source links are references, never fetched by this checker.
    return ((path.parent / unquote(parsed.path)).resolve() if parsed.path else path), parsed.fragment


def check(path):
    text, meta, revision, anchors, links = inspect(path)
    result = {'id': meta['id'], 'revision': revision, 'anchors': len(anchors), 'links': len(links)}
    if path.suffix == '.html':
        page = Page(text)
        if not page.headings or not all(page.headings + page.figures):
            raise Refusal('every h2 and figure needs an explicit data-brief-anchor')
        loose = unanchored_decisions(CONTENT.search(text)[1])
        if loose:
            raise Refusal(UNANCHORED + '; '.join(d['title'] or '(untitled ' + d['tag'] + ')' for d in loose))
        notes(text)
        # Only a built page has a recorded revision to compare; Markdown leaves this undeclared.
        result['edited_since_build'] = revision != meta['revision']
        result['wallpaper'] = wall_label(text)
        result['comment_targets'] = len(page.targets)
        region = CONTENT.search(text)[1]
    else:
        region = markdown_prose(text)
        # Explicit ids and ^blocks survive a heading rename; heading slugs, also counted in anchors, do not.
        result['comment_targets'] = len(set(Page(region).ids + block_ids(region)))
    if UNWRITTEN in region:
        raise Refusal(f'unwritten scaffold passage: replace every {UNWRITTEN} before delivery')
    for parent in meta.get('derives_from') or []:
        if not (path.parent / parent).is_file():
            raise Refusal(f'derives_from names {parent}, which is not there; fix the path or drop it')
    if path.suffix == '.html' and not meta.get('calls'):
        rows = RowSpans(CONTENT.search(text)[1]).spans
        if rows:
            raise Refusal('the brief shows call rows but names no calls ledger: ' + ', '.join(sorted(rows)) +
                          '. Name it with "calls" in the identity comment')
    if meta.get('calls'):
        ledger = path.parent / meta['calls']
        data = load_ledger(ledger)
        spans = RowSpans(CONTENT.search(text)[1]) if path.suffix == '.html' else None
        shown = spans.spans if spans else {}
        known = {c['id'] for c in data['calls']}
        strangers = sorted(set(shown) - known) + (spans.nested if spans else [])
        if strangers:
            raise Refusal('call rows that are not one call of the ledger, or sit inside another row: ' + ', '.join(strangers))
        problems = {c['id']: row_problem(c, shown[c['id']], ledger.resolve().parent, path.resolve().parent)
                    for c in data['calls'] if c['id'] in shown}
        stale = [f'{name} ({problem})' for name, problem in problems.items() if problem]
        if stale:
            raise Refusal('call rows rendered from other words or evidence than the ledger holds: ' + ', '.join(stale) +
                          f'. Paste a fresh `brief.py calls {meta["calls"]} --html` and rebuild')
        # A waiting call needs a complete row of its own inside the brief, not merely an element with its id.
        waiting = [c['id'] for c in data['calls'] if c['status'] in ('open', 'asked')
                   and c['id'] not in (shown if path.suffix == '.html' else anchors)]
        if waiting:
            raise Refusal('open calls missing from the brief: ' + ', '.join(waiting) +
                          f'. Paste the section `brief.py calls {meta["calls"]} --html` prints into the source and rebuild')
        result['calls'] = {'ledger': meta['calls'], 'open': sum(c['status'] == 'open' for c in data['calls']),
                           'asked': sum(c['status'] == 'asked' for c in data['calls'])}
    external, broken = 0, []
    for link in links:
        target, fragment = local_target(path, link)
        if target is None:
            external += 1
            continue
        if not target.exists():
            broken.append(f'broken local link: {link}')
        elif fragment:
            if target == path:
                target_anchors = anchors
            elif target.suffix in ('.md', '.html') and target.is_file():
                target_anchors = anchors_in(read(target))
            else:
                broken.append(f'cannot verify local fragment: {link}')
                continue
            if unquote(fragment).removeprefix('^') not in target_anchors:
                broken.append(f'broken local fragment: {link}')
    if broken:
        # Every bad link at once, so one pass of fixes is enough.
        raise Refusal('; '.join(dict.fromkeys(broken)))
    result['external_links_unchecked'] = external
    return result


def atomic(path, text, expected=None):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix='.' + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(text.encode())
        # mkstemp makes 0600; a page is for readers, so keep its old mode or honour the umask.
        if path.exists():
            os.chmod(temporary, path.stat().st_mode & 0o777)
        else:
            umask = os.umask(0)
            os.umask(umask)
            os.chmod(temporary, 0o666 & ~umask)
        # Refuse a concurrent annotation write instead of replacing it with an older copy.
        actual = path.read_bytes() if path.exists() else None
        if actual != expected:
            raise Refusal('output changed during build; retry from the latest file')
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def kit_missing():
    return [str(path) for path in KIT_FILES if not path.is_file()]


WALL_IMAGE = re.compile(r'url\("data:image/[a-z+]+;base64,[A-Za-z0-9+/=]{100,}')


def wall_of(text):
    """The page's real <style id="brief-wall"> element, if it carries a wallpaper image.

    base.css names that element inside its own comment, so a text search finds the comment
    first; only the parser sees the element itself. An imageless wall is damage, not a wall."""
    for attrs, start, content_start, content_end, end in reversed(Scripts(text or '', 'style').blocks):
        if attrs.get('id') == 'brief-wall':
            return text[start:end] if WALL_IMAGE.search(text[content_start:content_end]) else None
    return None


def wall_label(text):
    wall = wall_of(text)
    if not wall:
        return None
    named = re.search(r'brief-wall — theme ([^\s(]+)[^,]*, wallpaper ([^\s·*]+)', wall)
    return f'{named[1]} · {named[2]}' if named else 'unlabelled wallpaper'


def pick_wall(old, no_wall):
    """Keep the wallpaper a rebuilt page already wears; otherwise ask brief-wall for one."""
    if no_wall:
        return ''
    kept = wall_of(old)
    if kept:
        return kept
    try:
        return subprocess.run([sys.executable, str(KIT / 'scripts/brief-wall')], check=True, capture_output=True, text=True).stdout
    except subprocess.CalledProcessError as exc:
        reason = ((exc.stderr or '').strip().splitlines() or ['brief-wall failed without a message'])[-1]
        raise Refusal(f'no wallpaper: {reason}. Pass --no-wall to build on the stylesheet\'s own dark ground, '
                      'outside the Omarchy house look.') from exc


def source_identity(text, identity, purpose):
    """Fill --id and --purpose from the source's own <!-- briefing: {...} --> comment, which `new` writes."""
    found = [json.loads(match[1]) for match in META.finditer(text)]
    if len(found) > 1 or (found and not isinstance(found[0], dict)):
        raise Refusal('a source carries at most one <!-- briefing: {...} --> identity object')
    declared = found[0] if found else {}
    for name, given in (('id', identity), ('purpose', purpose)):
        if given and declared.get(name) and given != declared[name]:
            raise Refusal(f'--{name} {given} disagrees with the source, which says {declared[name]}; change one of them')
    identity, purpose = identity or declared.get('id'), purpose or declared.get('purpose')
    if not identity or not purpose:
        raise Refusal('name the brief: pass --id and --purpose, or put <!-- briefing: {"id": "…", "purpose": "…"} --> in the source')
    return identity, purpose


def source_extras(text):
    """What a source says beyond its name: the briefs it derives from and the calls ledger it shows."""
    found = [json.loads(match[1]) for match in META.finditer(text)]
    declared = found[0] if found and isinstance(found[0], dict) else {}
    extras = {}
    parents = declared.get('derives_from')
    if parents is not None:
        if not isinstance(parents, list) or not all(isinstance(p, str) and p for p in parents):
            raise Refusal('derives_from must be a list of relative paths to the briefs this one builds on')
        extras['derives_from'] = parents
    if declared.get('calls') is not None:
        if not isinstance(declared['calls'], str) or not declared['calls'].endswith('.calls.json'):
            raise Refusal('calls must name a .calls.json ledger, relative to the brief')
        extras['calls'] = declared['calls']
    return extras


def build(source, output, identity=None, purpose=None, no_wall=False, discard_edits=False):
    if output.suffix != '.html':
        raise Refusal('HTML output needs an .html suffix')
    if source.resolve() == output.resolve():
        raise Refusal('source and output must be different files')
    missing = kit_missing()
    if missing:
        raise Refusal('an HTML build reads the decision-brief and fancy-pants-brief skills installed beside this one, '
                      'and these are missing: ' + ', '.join(missing) + '. Install the three directories side by side, '
                      'or write the brief in Markdown, which needs nothing else. `brief.py doctor` lists what works here.')
    text = read(source)
    identity, purpose = source_identity(text, identity, purpose)
    for slot in ('head', 'notes'):
        if text.count('<!--BRIEF:' + slot + '-->') != 1:
            raise Refusal(f'expected one BRIEF:{slot} placeholder')
    if re.findall(r'<!--BRIEF:(.*?)-->', text) != ['head', 'notes']:
        raise Refusal('unknown or out-of-order BRIEF placeholder')
    regions = list(CONTENT.finditer(text))
    if len(regions) == 1 and '<!--BRIEF:' in regions[0][1]:
        # Assembled UI inside the fingerprinted region would make every feedback map stale at birth.
        raise Refusal('keep <!--BRIEF:head--> and <!--BRIEF:notes--> outside the <!--brief-content--> region')
    page = Page(text)
    if len(page.ids) != len(set(page.ids)) or not page.headings or not all(page.headings + page.figures):
        raise Refusal('source needs unique ids and explicit anchors on every h2 and figure')
    loose = unanchored_decisions(regions[0][1]) if len(regions) == 1 else []
    if loose:
        raise Refusal(UNANCHORED + '; '.join(d['title'] or '(untitled ' + d['tag'] + ')' for d in loose))
    extras = source_extras(text)
    # The identity names its parents and ledger relative to the source; the page may be built elsewhere.
    for key in ('derives_from', 'calls'):
        if key in extras:
            rebase = lambda rel: os.path.relpath((source.parent / rel).resolve(), output.resolve().parent)
            extras[key] = [rebase(rel) for rel in extras[key]] if key == 'derives_from' else rebase(extras[key])
    if not SAFE_ID.fullmatch(identity):
        raise Refusal('use a stable lowercase brief id')
    old_bytes = output.read_bytes() if output.exists() else None
    old = old_bytes.decode('utf-8') if old_bytes is not None else None
    previous = None
    retained = None
    edited = False
    if old is not None:
        try:
            _, previous = island(old, 'brief-meta')
        except Refusal as exc:
            raise Refusal(f'{output} exists and was not built by this helper ({exc}); choose another output path or move that file aside') from exc
        if previous.get('id') != identity:
            raise Refusal(f"{output.name} belongs to the brief {previous.get('id')!r}, not {identity!r}; "
                          f"build with --id {previous.get('id')} or choose another output path")
        retained = notes(old)
        try:
            edited = content_revision(old, identity, previous.get('purpose')) != previous.get('revision')
        except Refusal:
            edited = True
        if edited and not discard_edits:
            # A rebuild from the source would silently drop an edit made to the page itself.
            raise Refusal(f'{output.name} was edited after its last build and {source.name} does not have those edits. '
                          'Port them into the source and rebuild, or pass --discard-output-edits '
                          '(the edited page is kept as a .bak file).')
    base = read(KIT / 'assets/base.css')
    note_css = read(KIT / 'assets/notes.css')
    note_js = read(KIT / 'assets/notes.js')
    # Checked adaptation at assembly time; the shared source and wire format stay intact.
    hook = 'if (num) id = pre + \'-\' + num.textContent.trim();'
    if note_js.count(hook) != 1:
        raise Refusal('canonical notes anchor hook changed; review the adapter before building')
    note_js = note_js.replace(hook, "if (el.hasAttribute('data-brief-anchor')) id = el.getAttribute('data-brief-anchor');\n        else " + hook)
    if '</script' in note_js.lower():
        raise Refusal('canonical notes script contains a closing script tag')
    wall = pick_wall(old, no_wall)
    revision = content_revision(text, identity, purpose)
    meta = {'id': identity, 'medium': 'html', 'purpose': purpose, 'revision': revision, 'feedback_revision': revision,
            'built_at': utc_minute(), **extras}
    if previous and previous.get('revision') == revision and not edited and previous.get('built_at'):
        meta['built_at'] = previous['built_at']  # same content: stamps made since the last build still count
    if previous and retained and (retained[1] or retained[3]):
        # Format 1 has no per-comment content revision. Mixed-origin notes stay unknown, and so
        # do notes a reader left on hand-edited wording that this rebuild discards.
        same = previous.get('revision') == revision and not edited
        meta['feedback_revision'] = previous.get('feedback_revision') if same else None
    markup = read(KIT / 'reference/notes-markup.html')
    markup = re.sub(r'<script>.*?</script>', '', markup, flags=re.S)
    if retained:
        for name, block in (('report-notes', retained[0]), ('report-concurs', retained[2])):
            empty, _ = island(markup, name)
            markup = markup.replace(empty, block, 1)
        mirrors = list(MIRROR.finditer(old))
        if mirrors:
            markup += mirrors[-1][0]
    head = '<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="dark">'
    head += '<style>' + base + '</style>' + wall + '<style>' + note_css + '</style>'
    config = '<script>window.NOTES_TARGETS=[[\'[data-brief-anchor]\',\'brief\']];window.NOTES_FILE=' + encode(identity + '-' + revision[7:19]) + ';</script>'
    payload = markup + '<script type="application/json" id="brief-meta">' + encode(meta) + '</script>' + config
    bridge = read(HERE / 'assets/feedback-bridge.js')
    if '</script' in bridge.lower():
        raise Refusal('feedback bridge contains a closing script tag')
    payload += '<script>' + read(LOOK / 'assets/sections.js') + '</script><script>' + note_js + '</script><script>' + bridge + '</script>'
    text = text.replace('<!--BRIEF:head-->', head).replace('<!--BRIEF:notes-->', payload)
    # Validate all islands, and that the page fingerprints as its source did, before touching the previous file.
    notes(text)
    if content_revision(text, identity, purpose) != revision:
        raise Refusal('the assembled page fingerprints differently from its source; report this with the source attached')
    if old_bytes is not None:
        backup = output.with_name(output.name + '.' + hashlib.sha256(old_bytes).hexdigest()[:12] + '.bak')
        if backup.exists() and backup.read_bytes() != old_bytes:
            raise Refusal('backup collision')
        if not backup.exists():
            with backup.open('xb') as stream:
                stream.write(old_bytes)
    atomic(output, text, old_bytes)
    result = {'output': str(output), 'id': identity, 'revision': revision, 'feedback_revision': meta['feedback_revision'],
              'wallpaper': wall_label(text)}
    if old_bytes is not None:
        result['backup'] = str(backup)
    if edited:
        result['discarded_output_edits'] = str(backup)
    return result


def feedback(path):
    text, meta, revision, anchors, _ = inspect(path)
    comments = []
    if path.suffix == '.html':
        _, nm, _, cm = notes(text)
        for anchor, entries in nm.items():
            for entry in entries:
                comments.append({'target': anchor, 'title': entry.get('title'), 'text': entry['text'], 'stance': None, 'at': entry.get('ts')})
        comments += [{'target': a, 'title': None, 'text': '', 'stance': 'agree', 'at': ts} for a, ts in cm.items()]
        revision = meta.get('feedback_revision')
    return {'format': 1, 'document': {'id': meta['id'], 'href': path.name, 'medium': meta['medium'], 'revision': revision}, 'comments': comments}


def copy_map(text):
    """Normalize the HTML reader's Copy map text into a format-1 envelope, or None if it is not one.

    The text names the page as `NOTES — <id>-<first 12 hex of its revision>`. It joins each note's
    lines with spaces, so the envelope says so rather than passing the words off as exact."""
    text = text.lstrip('\ufeff').replace('\r\n', '\n').replace('\r', '\n')
    head = re.search(r'^NOTES — (\S+)[ \t]*$', text, re.M)
    if not head and re.search(r'^> ?NOTES — ', text, re.M):
        text = re.sub(r'^> ?', '', text, flags=re.M)  # pasted as a chat quote
        head = re.search(r'^NOTES — (\S+)[ \t]*$', text, re.M)
    if not head:
        return None
    named = re.fullmatch(r'(.+)-([0-9a-f]{12})', head[1])
    identity, revision = (named[1], 'sha256:' + named[2]) if named else (head[1], None)
    # The bridge marks carried notes; a READER NOTES mirror inside a saved file never carries that mark.
    if 'REVISION UNKNOWN' in text or 'READER NOTES —' in text[:head.start()]:
        revision = None
    comments, target, title, orphan = [], None, None, False
    for line in text[head.end():].splitlines():
        block = re.match(r'\[([^\]\s]+)\] ?(.*)$', line)
        if block:
            target, rest = block[1], block[2].rstrip()
            orphan = rest.startswith('UNRESOLVED')
            title = None if orphan else (rest.removesuffix('✓ concur').rstrip() or None)
            if not orphan and rest.endswith('✓ concur'):
                comments.append({'target': target, 'title': title, 'text': '', 'stance': 'agree', 'at': None})
            continue
        note = re.match(r'  · (.*)$', line)
        if note and target:
            if orphan and note[1] == 'Explicit agreement on the earlier target':
                comments.append({'target': target, 'title': None, 'text': '', 'stance': 'agree', 'at': None})
            else:
                comments.append({'target': target, 'title': title, 'text': note[1], 'stance': None, 'at': None})
        elif not line.strip():
            target = None
    # `revision` is a full fingerprint by contract; the copy carries only its first twelve hex digits.
    return {'format': 1, 'document': {'id': identity, 'href': None, 'medium': 'html', 'revision': None, 'revision_prefix': revision},
            'comments': comments, 'source': 'copy-map', 'text_exact': False}


def load_map(map_path):
    raw = sys.stdin.read() if str(map_path) == '-' else read(map_path)
    try:
        return json.loads(raw)
    except ValueError:
        data = copy_map(raw)
        if data is None:
            raise Refusal('the map is neither a format-1 JSON envelope nor Copy map text, which starts with a line '
                          '"NOTES — <brief id>-<12 hex>". Ask the reader to press copy map again and paste all of it, '
                          'or read their message as ordinary feedback against the brief')
        return data


def resolve(path, map_path):
    _, meta, revision, anchors, _ = inspect(path)
    data = load_map(map_path)
    if not isinstance(data, dict) or data.get('format') != 1:
        raise Refusal('unsupported feedback format')
    doc, comments = data.get('document'), data.get('comments')
    if not isinstance(doc, dict) or not isinstance(comments, list):
        raise Refusal('feedback needs document and comments')
    common = []
    if doc.get('id') != meta['id'] or doc.get('medium') != meta['medium']:
        common.append('document_mismatch')
    claimed, prefix = doc.get('revision'), doc.get('revision_prefix')
    if claimed is None and not prefix:
        common.append('revision_unknown')
    elif (claimed is not None and claimed != revision) or (claimed is None and not revision.startswith(prefix)):
        # A Copy map carries only the first twelve hex digits of the revision it was made on.
        common.append('revision_changed')
    for comment in comments:
        if not isinstance(comment, dict) or not isinstance(comment.get('target'), str) or not isinstance(comment.get('text'), str):
            raise Refusal('each comment needs a target and text')
        if comment.get('stance') not in (None, 'agree', 'disagree'):
            raise Refusal('invalid stance; unknown is null')
        if not comment['text'].strip() and comment.get('stance') is None:
            raise Refusal('empty comment without an explicit stance')
        reasons = common + ([] if comment['target'] in anchors else ['target_missing'])
        comment['status'] = 'needs_review' if reasons else 'resolved'
        comment['reasons'] = reasons
    data['current_revision'] = revision
    return data


def utc_minute(now=None):
    return (now or datetime.now(timezone.utc)).strftime('%Y-%m-%dT%H:%MZ')


def file_digest(path):
    return 'sha256:' + hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_ledger(path, create=False):
    """A calls ledger: format 1, one JSON object, the calls in the order they were made."""
    if not path.name.endswith('.calls.json'):
        raise Refusal(f'{path.name}: a calls ledger is named NAME.calls.json, so no other JSON is ever written over')
    if not path.exists():
        if create:
            return {'format': 1, 'kind': 'briefing-calls', 'calls': []}
        raise Refusal(f'{path}: no calls ledger here; `brief.py call {path} …` starts one')
    try:
        data = json.loads(read(path))
    except ValueError as exc:
        raise Refusal(f'{path}: malformed JSON; repair it by hand, nothing was written') from exc
    if not isinstance(data, dict) or data.get('format') != 1 or data.get('kind') != 'briefing-calls':
        raise Refusal(f'{path}: not a format-1 briefing calls ledger')
    calls = data.get('calls')
    if not isinstance(calls, list):
        raise Refusal(f'{path}: every call needs an id and a known status')
    seen = set()
    for call in calls:
        problem = call_problem(call)
        if problem:
            raise Refusal(f'{path}: every call needs an id and a known status, and {problem}; repair it by hand')
        if call['id'] in seen:
            raise Refusal(f'{path}: {call["id"]} appears twice; repair it by hand')
        seen.add(call['id'])
    return data


def call_problem(call):
    """What makes a ledger entry unusable, or None. A missing field is never read as an empty one."""
    if not isinstance(call, dict) or not isinstance(call.get('id'), str) or not CALL_ID.fullmatch(call['id']):
        return 'a call has no valid call-… id'
    name = call['id']
    if call.get('status') not in ('open', 'asked') + VERDICTS:
        return f'{name} has no known status'
    for field in ('title', 'chose', 'because'):
        if not isinstance(call.get(field), str) or not call[field].strip():
            return f'{name} has no {field}'
    judgement = call.get('judgement')
    if (not isinstance(judgement, dict) or judgement.get('uncertainty') not in LEVELS or judgement.get('taste') not in LEVELS
            or judgement.get('posture') not in POSTURES or judgement.get('outcome') not in ('carry-on', 'stop')
            or not isinstance(judgement.get('slot'), str) or not isinstance(judgement.get('filled_by'), str)
            or not isinstance(judgement.get('posture_declared'), bool)
            or not isinstance(judgement.get('reasons'), list) or not all(isinstance(r, str) for r in judgement['reasons'])):
        return f'{name} has no complete judgement'
    evidence = call.get('evidence')
    if not isinstance(evidence, list) or not all(
            isinstance(e, dict) and isinstance(e.get('href'), str) and e['href'] and SHA256.fullmatch(str(e.get('sha256')))
            and e.get('kind') in EVIDENCE_KINDS for e in evidence):
        return f'{name} has malformed evidence'
    for field in ('passed_over', 'conflicts'):
        if not isinstance(call.get(field), list) or not all(isinstance(x, str) for x in call[field]):
            return f'{name} has malformed {field}'
    for field in ('context', 'undo'):
        if call.get(field) is not None and not isinstance(call[field], str):
            return f'{name} has a malformed {field}'
    if call.get('door') not in (None, 'two-way', 'one-way') or not isinstance(call.get('at'), str):
        return f'{name} has a malformed door or time'
    if call.get('under') is not None and not (isinstance(call['under'], dict) and isinstance(call['under'].get('href'), str)):
        return f'{name} has a malformed under'
    verdict = call.get('verdict')
    if call['status'] in VERDICTS:
        if (not isinstance(verdict, dict) or not isinstance(verdict.get('at'), str) or not verdict['at']
                or not isinstance(verdict.get('via'), str) or not verdict['via']):
            return f'{name} says {call["status"]} with no verdict recorded: who answered, and when'
        if call['status'] == 'overturned' and not (isinstance(verdict.get('note'), str) and verdict['note'].strip()):
            return f'{name} is overturned with no reason recorded'
    elif verdict is not None:
        return f'{name} is {call["status"]} but carries a verdict'
    return None


@contextmanager
def ledger_lock(path):
    """Serialize read, change and replace of one ledger across processes."""
    path.parent.mkdir(parents=True, exist_ok=True)
    handle = os.open(str(path) + '.lock', os.O_CREAT | os.O_RDWR, 0o644)
    try:
        if fcntl:
            fcntl.flock(handle, fcntl.LOCK_EX)
        yield
    finally:
        if fcntl:
            fcntl.flock(handle, fcntl.LOCK_UN)
        os.close(handle)


def call_fingerprint(call):
    """What a reader judged when they stamped a call's row: its words and its evidence's bytes."""
    seen = {k: call.get(k) for k in ('id', 'title', 'context', 'chose', 'because', 'passed_over', 'undo', 'door')}
    seen['evidence'] = [e['sha256'] for e in call.get('evidence') or []]
    return digest(json.dumps(seen, ensure_ascii=False, sort_keys=True))


class RowSpans(HTMLParser):
    """The exact markup of each call row on a page, by id, so it can be compared with a fresh rendering."""
    def __init__(self, text):
        super().__init__()
        self.text, self.spans, self.current, self.depth, self.start, self.nested = text, {}, None, 0, 0, []
        self.offsets = [0] + [match.end() for match in re.finditer('\n', text)]
        self.feed(text)

    def position(self):
        line, column = self.getpos()
        return self.offsets[line - 1] + column

    def handle_starttag(self, tag, attributes):
        if tag in VOID_TAGS:
            return
        attrs = dict(attributes)
        if self.current:
            self.depth += 1
            if attrs.get('data-call'):
                self.nested.append(attrs.get('id') or '(unnamed)')
            return
        if attrs.get('data-call') and attrs.get('id'):
            self.current, self.depth, self.start = attrs['id'], 1, self.position()

    def handle_endtag(self, tag):
        if not self.current:
            return
        self.depth -= 1
        if self.depth == 0:
            self.spans.setdefault(self.current, self.text[self.start:self.text.find('>', self.position()) + 1])
            self.current = None


def row_problem(call, row, base, where):
    """Why a page's row does not show this call, or None.

    A row is generated, never written, so it must be one of the renderings the ledger can produce: the
    call as it is, under any status it could have had when the row was printed. Anything added, removed
    or reworded, in words, pictures, links or structure, is a mismatch."""
    if row is None:
        return 'not shown'
    flat = lambda html: html.replace('\r\n', '\n').replace('\r', '\n')  # a browser save drops carriage returns
    shown = Content(flat(row), keep_nav=True)
    if shown.duplicated:
        return 'a tag in it repeats an attribute, so what a browser shows is not what was checked'
    held = {}
    for src in re.findall(r'src="data:[^";,]+;base64,([A-Za-z0-9+/=]+)"', row):
        raw = base64.b64decode(src)
        held['sha256:' + hashlib.sha256(raw).hexdigest()] = raw
    refused = []
    tries = [(status, None) for status in ('open', 'asked', 'concurred', 'overturned', 'superseded')]
    tries += [(status, moved) for status in ('overturned', 'superseded') for moved in ('changed', 'missing')]
    for status, moved in tries:
        try:
            candidate = render_row(call, base, where, status, None if moved else held, moved)
        except Refusal as exc:
            refused.append(str(exc))
            continue
        if Content(flat(candidate), keep_nav=True).parts == shown.parts:
            return None
    if len(refused) == 3:
        return 'its evidence changed or went missing since it was recorded'
    return 'what it shows differs from what the ledger renders'


def save_ledger(path, data, before):
    atomic(path, json.dumps(data, ensure_ascii=False, indent=2) + '\n', before)


def decide(uncertainty, taste, posture, door=None, conflicts=()):
    """Carry on, or stop and ask. Code applies the policy; the judgement slot supplies the two readings.

    Any posture stops on a contradicted approval, a one-way door, an unreadable reading, or a high one.
    Standard also stops when both readings are medium; strict stops on any medium."""
    reasons = []
    if conflicts:
        reasons.append('it contradicts a decision the person approved: ' + ', '.join(conflicts))
    if door == 'one-way':
        reasons.append('it is a one-way door: undoing it needs the person')
    readings = {'uncertainty': uncertainty, 'taste': taste}
    unknown = [name for name, level in readings.items() if level == 'unknown']
    high = [name for name, level in readings.items() if level == 'high']
    medium = [name for name, level in readings.items() if level == 'medium']
    if unknown:
        reasons.append('the judgement slot could not read its ' + ' or '.join(unknown))
    if high:
        reasons.append(' and '.join(high) + (' is' if len(high) == 1 else ' are') + ' high')
    elif posture == 'standard' and len(medium) == 2:
        reasons.append('both readings are medium under a standard posture')
    elif posture == 'strict' and medium:
        reasons.append(' and '.join(medium) + (' is' if len(medium) == 1 else ' are') + ' medium under a strict posture')
    return ('stop' if reasons else 'carry-on'), reasons


def brief_pointer(brief, relative_to):
    _, meta, revision, _, _ = inspect(brief)
    return {'href': os.path.relpath(brief.resolve(), relative_to.resolve()), 'id': meta['id'], 'revision': revision}


def record_call(ledger, title, chose, because, uncertainty, taste, posture=None, context=None, passed_over=(),
                undo=None, door=None, evidence=(), conflicts=(), under=None, call_id=None, filled_by='agent', now=None):
    """Write down a decision nobody made, apply the policy, and say whether to carry on."""
    for name, value in (('title', title), ('chose', chose), ('because', because)):
        if not (value or '').strip():
            raise Refusal(f'a call needs --{name}: what was decided, what was chosen and why, in plain words')
    for name, level in (('uncertainty', uncertainty), ('taste', taste)):
        if level not in LEVELS:
            raise Refusal(f'--{name} is one of {", ".join(LEVELS)}; the slot {SLOT} supplies it')
    if posture is not None and posture not in POSTURES:
        raise Refusal('--posture is one of ' + ', '.join(POSTURES) + '; read it from the project\'s instructions')
    if door not in (None, 'two-way', 'one-way'):
        raise Refusal('--door is two-way (cheap to undo) or one-way (undoing it needs the person)')
    with ledger_lock(ledger):
        return _record_call(ledger, title, chose, because, uncertainty, taste, posture, context, passed_over, undo, door,
                            evidence, conflicts, under, call_id, filled_by, now)


def _record_call(ledger, title, chose, because, uncertainty, taste, posture, context, passed_over, undo, door,
                 evidence, conflicts, under, call_id, filled_by, now):
    before = ledger.read_bytes() if ledger.exists() else None
    data = load_ledger(ledger, create=True)
    call_id = call_id or 'call-' + re.sub(r'[^a-z0-9]+', '-', title.lower()).strip('-')[:48].rstrip('-')
    if not CALL_ID.fullmatch(call_id):
        raise Refusal('a call id looks like call-short-name, in lowercase')
    if any(c['id'] == call_id for c in data['calls']):
        raise Refusal(f'{call_id} is already in the ledger; pass --id for a new call, or `verdict … superseded` the old one')
    base = ledger.resolve().parent
    proof = []
    for item in evidence:
        item = Path(item)
        if not item.is_file():
            raise Refusal(f'evidence {item} is not a file; a call records what it was decided on, so that file must exist')
        proof.append({'href': os.path.relpath(item.resolve(), base), 'sha256': file_digest(item),
                      'kind': 'screenshot' if item.suffix.lower() in IMAGE_TYPES else 'file'})
    outcome, reasons = decide(uncertainty, taste, posture or 'standard', door, conflicts)
    entry = {'id': call_id, 'at': utc_minute(now), 'title': title.strip(), 'context': context, 'chose': chose.strip(),
             'because': because.strip(), 'passed_over': [p for p in passed_over if p.strip()], 'undo': undo, 'door': door,
             'evidence': proof, 'conflicts': list(conflicts),
             'under': brief_pointer(Path(under), base) if under else None,
             'judgement': {'slot': SLOT, 'filled_by': filled_by, 'uncertainty': uncertainty, 'taste': taste,
                           'posture': posture or 'standard', 'posture_declared': posture is not None,
                           'outcome': outcome, 'reasons': reasons},
             'status': 'open' if outcome == 'carry-on' else 'asked', 'verdict': None}
    data['calls'].append(entry)
    save_ledger(ledger, data, before)
    return {'ledger': str(ledger), 'id': call_id, 'outcome': outcome, 'status': entry['status'], 'reasons': reasons,
            'next': 'carry on, and name this call in your reply as a decision you made for the person to concur with'
            if outcome == 'carry-on' else 'stop: ask the person before acting on it, with the choice you would make'}


def evidence_states(ledger, call):
    states = []
    for item in call.get('evidence') or []:
        target = ledger.resolve().parent / item['href']
        states.append('missing' if not target.is_file() else 'same' if file_digest(target) == item['sha256'] else 'changed')
    return states


def calls_report(ledger):
    """Every call, what it still needs, and whether the work has settled: nothing open, asked or stale."""
    data = load_ledger(ledger)
    rows = []
    for call in data['calls']:
        states = evidence_states(ledger, call)
        needs = []
        if call['status'] == 'open':
            needs.append('a concur or an overturn')
        elif call['status'] == 'asked':
            needs.append('an answer before the agent acts')
        # An overturned call's evidence is meant to change, and a superseded one no longer stands.
        if call['status'] in ('open', 'asked', 'concurred') and any(s != 'same' for s in states):
            needs.append('evidence ' + ', '.join(sorted(set(s for s in states if s != 'same'))) + ' since it was recorded')
        rows.append({'id': call['id'], 'title': call['title'], 'status': call['status'], 'evidence': states, 'needs': needs})
    return {'ledger': str(ledger), 'settled': not any(r['needs'] for r in rows), 'calls': rows,
            'open': sum(r['status'] == 'open' for r in rows), 'asked': sum(r['status'] == 'asked' for r in rows)}


def record_verdict(ledger, call_id, verdict, note=None, follow_up=None, superseded_by=None, now=None):
    """Record the person's answer to a call: concurred, overturned (with their reason) or superseded."""
    if verdict not in VERDICTS:
        raise Refusal('a verdict is concurred, overturned or superseded')
    if verdict == 'overturned' and not (note or '').strip():
        raise Refusal('an overturn carries the person\'s reason in --note; it is what the follow-up is built from')
    with ledger_lock(ledger):
        return _record_verdict(ledger, call_id, verdict, note, follow_up, superseded_by, now)


def _record_verdict(ledger, call_id, verdict, note, follow_up, superseded_by, now):
    before = ledger.read_bytes() if ledger.exists() else None
    data = load_ledger(ledger)
    call = next((c for c in data['calls'] if c['id'] == call_id), None)
    if call is None:
        raise Refusal(f'{call_id} is not in {ledger.name}; `brief.py calls {ledger}` lists the ids')
    call['status'] = verdict
    call['verdict'] = {'at': utc_minute(now), 'via': 'reported', 'note': note, 'follow_up': follow_up,
                       'superseded_by': superseded_by, 'seen': [e['sha256'] for e in call.get('evidence') or []]}
    save_ledger(ledger, data, before)
    return {'ledger': str(ledger), 'id': call_id, 'status': verdict, 'settled': calls_report(ledger)['settled']}


def utc_of(stamp):
    """A stamp as a UTC minute, or None when it cannot be read. A stamp with no zone is UTC, as notes.js writes it."""
    if not isinstance(stamp, str):
        return None
    text = stamp.strip().replace(' ', 'T', 1)
    text = text[:-1] + '+00:00' if text.endswith('Z') else text
    try:
        moment = datetime.fromisoformat(text)
    except ValueError:
        return None
    moment = moment.replace(tzinfo=timezone.utc) if moment.tzinfo is None else moment.astimezone(timezone.utc)
    return moment.replace(second=0, microsecond=0)


def stamp_is_current(meta, revision, stamped_at):
    """Whether a stamp was made on the version of the page that exists now.

    True when the page's feedback revision is this revision. Otherwise a stamp counts only when it is
    stamped after the page was built, to the minute; a stamp in the build's own minute stays unknown."""
    if meta.get('feedback_revision') == revision:
        return True
    if meta.get('revision') != revision:
        return False  # edited by hand since its build: no stamp on it can be placed on a version
    built, stamped = utc_of(meta.get('built_at')), utc_of(stamped_at)
    return bool(built and stamped and stamped > built)


def sync_calls(ledger, page):
    """Take the CONCUR stamps a reader put on a brief's call rows into the ledger, and list their notes."""
    text, meta, revision, anchors, _ = inspect(page)
    if page.suffix != '.html':
        raise Refusal('sync reads the CONCUR stamps saved in a built HTML brief')
    _, saved_notes, _, concurs = notes(text)
    shown = RowSpans(CONTENT.search(text)[1]).spans
    where = page.resolve().parent
    with ledger_lock(ledger):
        before = ledger.read_bytes() if ledger.exists() else None
        data = load_ledger(ledger)
        concurred, to_answer, mismatched, stale_stamps = [], {}, [], []
        for call in data['calls']:
            said = [n['text'] for n in saved_notes.get(call['id'], [])]
            if call['id'] in concurs and call['status'] in ('open', 'asked') and row_problem(call, shown.get(call['id']), ledger.resolve().parent, where):
                # The reader stamped a row that does not show this call; it says nothing about this call.
                mismatched.append(call['id'])
                continue
            if call['id'] in concurs and call['status'] in ('open', 'asked') and not stamp_is_current(meta, revision, concurs[call['id']]):
                stale_stamps.append(call['id'])
                continue
            if call['id'] in concurs and call['status'] in ('open', 'asked'):
                call['status'] = 'concurred'
                call['verdict'] = {'at': concurs[call['id']], 'via': 'concur stamp', 'note': ' '.join(said) or None,
                                   'follow_up': None, 'superseded_by': None, 'brief': meta['id'],
                                   'revision': meta.get('feedback_revision'),
                                   'seen': [e['sha256'] for e in call.get('evidence') or []]}
                concurred.append(call['id'])
            elif said and call['status'] in ('open', 'asked'):
                to_answer[call['id']] = said
        if concurred:
            save_ledger(ledger, data, before)
    report = calls_report(ledger)
    return {'ledger': str(ledger), 'brief': meta['id'], 'concurred': concurred, 'notes_to_answer': to_answer,
            'mismatched': mismatched, 'stamped_on_another_version': stale_stamps, 'settled': report['settled'],
            'calls': report['calls']}


HEFT = {'low': 0, 'medium': 1, 'high': 2, 'unknown': 2}


def heft(call):
    return HEFT[call['judgement']['uncertainty']] + HEFT[call['judgement']['taste']]


def is_routine(call, status=None):
    return heft(call) == 0 and not call.get('evidence') and (status or call['status']) != 'asked'


def render_row(call, base, where, status=None, recorded=None, moved=None):
    """One call's stampable row, exactly as `calls --html` prints it; `status` overrides the ledger's.

    `recorded` maps an evidence hash to bytes a page already carries, so a row printed before its evidence
    moved can still be reproduced, and marks a comparison, where a link renders whatever its file holds now."""
    status = status or call['status']
    e = html_escape
    routine = is_routine(call, status)
    facts = [('Chose', call['chose']), ('Because', call['because'])]
    if call.get('context') and not routine:
        facts.insert(0, ('Context', call['context']))
    if call.get('passed_over') and not routine:
        facts.append(('Passed over', '; '.join(call['passed_over'])))
    if call.get('undo') and not routine:
        facts.append(('Undo', call['undo'] + (' (one-way door)' if call.get('door') == 'one-way' else '')))
    j = call['judgement']
    facts.append(('Judged', f"uncertainty {j['uncertainty']} · your taste {j['taste']} · {j['posture']} posture"
                  f"{'' if j.get('posture_declared') else ' (undeclared, so standard)'} · read by {j['filled_by']} in {j['slot']}"))
    figures = []
    for n, item in enumerate(call.get('evidence') or [], 1):
        source = base / item['href']
        short, kind = e(item['sha256'][7:19]), e(item['kind'])
        fid = f"fig-{call['id']}-{n}"
        state = 'missing' if not source.is_file() else 'same' if file_digest(source) == item['sha256'] else 'changed'
        mime = IMAGE_TYPES.get(source.suffix.lower())
        held = (recorded or {}).get(item['sha256'])
        if recorded is not None and state != 'same' and (held is not None or not mime) and not moved:
            state = 'held'  # reproducing a row printed earlier: its own bytes, or a link, stand for the file
        if moved and state != 'same':
            state = moved  # reproducing a retired row printed when its evidence was in that state
        if state not in ('same', 'held') and status in ('overturned', 'superseded'):
            # A retired call's evidence may move on; say so rather than show the new bytes as the old.
            figures.append(f'<figure id="{fid}" data-brief-anchor="{fid}"><span class="lbl">{e(call["title"])} · '
                           f'{kind} {short}</span><p>The recorded {kind}, {short}, is {state} since; it is not shown.</p>'
                           f'<figcaption><b>This call is {e(status)}, so its evidence was free to change.</b>'
                           f'</figcaption></figure>')
            continue
        if state == 'missing':
            raise Refusal(f"{item['href']} is missing; {call['id']} was decided on it, so restore it or supersede the call")
        if state == 'changed':
            raise Refusal(f"{item['href']} changed since {call['id']} was recorded; record the call again or supersede it")
        raw = held if held is not None else (source.read_bytes() if mime and source.is_file() else None)
        if mime and len(raw) > EMBED_LIMIT:
            raise Refusal(f"{item['href']} is over {EMBED_LIMIT // 1048576} MB; save a smaller screenshot and record the call again")
        if mime:
            body = (f'<img src="data:{mime};base64,{base64.b64encode(raw).decode()}" '
                    f'alt="{e("What the call was decided on: " + call["title"])}">')
        else:
            body = f'<p><a href="{e(quote(os.path.relpath(source, where)))}">{e(item["href"])}</a></p>'
        figures.append(f'<figure id="{fid}" data-brief-anchor="{fid}"><span class="lbl">{e(call["title"])} · '
                       f'{kind} {short}</span>{body}<figcaption><b>What I was looking at when I chose: '
                       f'{e(call["chose"])}.</b> A concur covers this {kind}, {short}, and no later one.'
                       f'</figcaption></figure>')
    # Green is for carrying on; a stop is a decision waiting, and red says so.
    if status == 'asked':
        stance = ('<p class="callout crit"><b>Stopped:</b> waiting on your answer, because ' + e('; '.join(j['reasons'])) + '.</p>')
    elif status == 'open':
        stance = '<p class="rec"><b>Meanwhile:</b> carrying on as if you concur.</p>'
    else:
        stance = f'<p class="rec"><b>{e(status.capitalize())}.</b></p>'
    dl = ''.join(f'<dt>{e(k)}</dt><dd>{e(v)}</dd>' for k, v in facts)
    return (f'<div class="ask call" id="{call["id"]}" data-brief-anchor="{call["id"]}" data-call="{call_fingerprint(call)}">'
            f'<h3>{e(call["title"])}</h3><dl class="facts">{dl}</dl>{"".join(figures)}{stance}</div>')


def render_calls(ledger, relative_to=None, number=None, every=False):
    """The closing brief's section of calls: one stampable row each, its evidence drawn in, nothing invented."""
    data = load_ledger(ledger)
    base = ledger.resolve().parent
    where = (relative_to or ledger).resolve().parent
    shown = [c for c in data['calls'] if every or c['status'] in ('open', 'asked')]
    # Stopped calls first, then the heaviest readings; routine calls, both readings low, last and compact.
    shown.sort(key=lambda c: (c['status'] != 'asked', is_routine(c), -heft(c)))
    rows = [render_row(call, base, where) for call in shown]
    e = html_escape
    asked = sum(c['status'] == 'asked' for c in shown)
    routine = sum(is_routine(c) for c in shown)
    lead = ('Nobody had decided these. I decided them and kept working' +
            (f', except {asked} I stopped to ask you about' if asked else '') +
            '. Stamp CONCUR on each one you agree with; a note on one is answered before the work settles.' +
            ((' The last one was routine, with both readings low, and shows' if routine == 1 else
              f' The last {routine} were routine, with both readings low, and show') + ' only what I chose and why.'
             if routine else ''))
    number = f'<span class="n">{e(number)}</span> ' if number else ''
    return (f'<section class="pane"><h2 id="sec-calls" data-brief-anchor="sec-calls">{number}Judgement calls</h2>\n'
            f'<p>{lead}</p>\n<div class="grill">\n' + '\n'.join(rows) + '\n</div></section>\n') if rows else ''


def standing(path, seen=None, depth=0, ledger=None):
    """What a brief has approved, what it only recommends, and the notes on it, followed up its derives_from.

    The calls ledger is the record for calls: a later overturn there outranks a stamp left on the page. A stamp
    made on another version of the page is listed for re-stamping, never counted as approval."""
    seen = seen if seen is not None else set()
    key = str(path.resolve())
    if key in seen:
        return {'href': str(path), 'state': 'already read'}
    seen.add(key)
    if not path.is_file():
        return {'href': str(path), 'state': 'unavailable: file missing'}
    text, meta, revision, anchors, _ = inspect(path)
    found, concurs, saved, current = [], {}, {}, True
    if path.suffix == '.html':
        found = Decisions(CONTENT.search(text)[1]).decisions()
        _, saved, _, concurs = notes(text)
        rows = RowSpans(CONTENT.search(text)[1]).spans
        current = lambda at: stamp_is_current(meta, revision, at)
    else:
        prose = markdown_prose(text)
        found = [{'anchor': a, 'title': h.strip()} for a, h in
                 re.findall(r'<a id=["\']((?:q|call)-[a-z0-9-]+)["\']\s*/?>(?:</a>)?\s*\n#{1,6}\s+(.+)', prose)]
        found += [{'anchor': a, 'title': h.strip()} for h, a in
                  re.findall(r'^#{1,6}\s+(.+?)\s+\^((?:q|call)-[a-z0-9-]+)[ \t]*$', prose, re.M)]
        rows = None
        side = path.with_name(path.name.removesuffix('.md') + '.feedback.json')
        same = False
        if side.is_file():
            try:
                envelope = json.loads(read(side))
            except ValueError as exc:
                raise Refusal(f'{side.name}: malformed JSON') from exc
            if (not isinstance(envelope, dict) or not isinstance(envelope.get('document'), dict)
                    or not isinstance(envelope.get('comments'), list)
                    or not all(isinstance(c, dict) and isinstance(c.get('target'), str) for c in envelope['comments'])):
                raise Refusal(f'{side.name}: expected a format-1 envelope with a document and a list of comments')
            same = envelope['document'].get('revision') == revision
            for c in envelope['comments']:
                if c.get('stance') == 'agree':
                    concurs[c['target']] = c.get('at') or ''
                elif c.get('text'):
                    saved.setdefault(c['target'], []).append({'text': c['text'], 'ts': c.get('at')})
    ledger = ledger or (path.parent / meta['calls'] if meta.get('calls') else None)
    ledger_state = 'none named' if ledger is None else ('read' if ledger.exists() else 'missing')
    verdicts = {c['id']: c for c in load_ledger(ledger)['calls']} if ledger_state == 'read' else {}
    if path.suffix != '.html':
        current = lambda at: same
    approved, unstamped, overturned, restamp, mismatched = [], [], [], [], []
    for d in found:
        anchor, title, call = d['anchor'], d['title'], verdicts.get(d['anchor'])
        status = call['status'] if call else None
        problem = row_problem(call, rows.get(anchor), ledger.resolve().parent, path.resolve().parent) if call and rows is not None else None
        if not call and rows is not None and anchor in rows:
            problem = "the ledger holds no such call"
        if problem:
            # The page shows something else under this call's name; neither its stamp nor the ledger's verdict is about it.
            mismatched.append({'anchor': anchor, 'title': title, 'problem': problem})
            continue
        if status == 'concurred':
            approved.append({'anchor': anchor, 'title': title, 'at': (call.get('verdict') or {}).get('at')})
        elif status == 'overturned':
            overturned.append({'anchor': anchor, 'title': title, 'note': (call.get('verdict') or {}).get('note')})
        elif status == 'superseded' or (status in ('open', 'asked') and anchor not in concurs):
            continue  # open and asked calls have their own lists in the read-out, at every depth
        elif anchor in concurs and current(concurs[anchor]):
            approved.append({'anchor': anchor, 'title': title, 'at': concurs[anchor]})
        elif anchor in concurs:
            restamp.append({'anchor': anchor, 'title': title, 'at': concurs[anchor]})
        else:
            unstamped.append({'anchor': anchor, 'title': title})
    # Answered calls whose rows a regenerated section left out still stand, or still stand overturned.
    listed = {d['anchor'] for d in found}
    for call in verdicts.values():
        if call['id'] in listed:
            continue
        if call['status'] == 'concurred':
            approved.append({'anchor': call['id'], 'title': call['title'], 'at': (call.get('verdict') or {}).get('at')})
        elif call['status'] == 'overturned':
            overturned.append({'anchor': call['id'], 'title': call['title'], 'note': (call.get('verdict') or {}).get('note')})
    open_calls = [{'id': c['id'], 'title': c['title'], 'judgement': c['judgement']} for c in verdicts.values() if c['status'] == 'open']
    asked_calls = [{'id': c['id'], 'title': c['title'], 'reasons': c['judgement']['reasons']}
                   for c in verdicts.values() if c['status'] == 'asked']
    said = [{'anchor': a, 'text': n['text'], 'at': n.get('ts')} for a, entries in saved.items() for n in entries]
    parents = [standing(path.parent / p, seen, depth + 1) if depth < 8 else {'href': p, 'state': 'not read: too deep'}
               for p in meta.get('derives_from') or []]
    report = calls_report(ledger) if ledger_state == 'read' else None
    return {'href': str(path), 'id': meta['id'], 'purpose': meta['purpose'], 'revision': revision, 'state': 'read',
            'calls': meta.get('calls'), 'ledger_state': ledger_state, 'ledger_path': str(ledger) if ledger else None,
            'ledger_settled': report['settled'] if report else None, 'approved': approved, 'unstamped': unstamped,
            'overturned': overturned, 'restamp': restamp, 'mismatched': mismatched, 'notes': said, 'parents': parents,
            'shows_calls': any((d['anchor'] or '').startswith('call-') for d in found),
            'open_calls': open_calls, 'asked_calls': asked_calls}


def settlement(tree):
    """yes, no or unknown, with the reasons. Unknown ancestry or an unread ledger is never reported as settled."""
    no, unknown = [], []
    def walk(node, top):
        if node['state'] == 'already read':
            return
        if node['state'] != 'read':
            unknown.append(f"{node['href']}: {node['state']}")
            return
        label = 'this brief' if top else node['id']
        if node['ledger_state'] == 'missing':
            unknown.append(f"{label}: its calls ledger {node['ledger_path']} is missing")
        elif node['ledger_state'] == 'none named' and (top or node.get('shows_calls')):
            unknown.append(f'{label} names no calls ledger' + ('' if top else ', yet shows call rows'))
        elif node['ledger_settled'] is False:
            no.append(f'{label}: calls still open, asked or stale')
        if node.get('mismatched'):
            unknown.append(f"{label}: rows that do not show their calls: " + ', '.join(m['anchor'] for m in node['mismatched']))
        for parent in node['parents']:
            walk(parent, False)
    walk(tree, True)
    return {'state': 'no' if no else 'unknown' if unknown else 'yes', 'why': no + unknown}


def orient(path, ledger=None):
    """The standing orders for work run under a brief: read at the start, after compaction, and before done."""
    tree = standing(path, ledger=ledger)
    if tree['state'] != 'read':
        raise Refusal(f'{path}: {tree["state"]}')
    tree['ledger'] = None
    if tree['ledger_state'] == 'read':
        ledger = Path(tree['ledger_path'])
        report = calls_report(ledger)
        by_id = {c['id']: c for c in load_ledger(ledger)['calls']}
        tree['ledger'] = {'href': str(ledger), 'settled': report['settled'],
                          'open': [{'id': r['id'], 'title': r['title'], 'judgement': by_id[r['id']]['judgement']}
                                   for r in report['calls'] if r['status'] == 'open'],
                          'asked': [{'id': r['id'], 'title': r['title'], 'reasons': by_id[r['id']]['judgement']['reasons']}
                                    for r in report['calls'] if r['status'] == 'asked'],
                          'stale': [{'id': r['id'], 'needs': r['needs']} for r in report['calls']
                                    if r['status'] in VERDICTS and r['needs']]}
    tree['settled'] = settlement(tree)
    return tree


def orient_text(tree):
    lines = [f"ORIENT  {tree['id']} · {tree['purpose']} · revision {tree['revision'][7:19]}"]
    def parents(node, indent='  '):
        for parent in node['parents']:
            if parent['state'] == 'read':
                lines.append(f"{indent}derives from  {parent['id']} ({parent['href']}) · revision {parent['revision'][7:19]}")
                parents(parent, indent + '  ')
            else:
                lines.append(f"{indent}derives from  {parent['href']} · {parent['state']}")
    parents(tree)
    def every(node, key):
        out = [(item, node['id']) for item in node.get(key, [])]
        for parent in node.get('parents', []):
            if parent['state'] == 'read':
                out += every(parent, key)
        return out
    def block(title, items, render):
        lines.append('')
        lines.append(title)
        lines.extend(render(*item) for item in items) if items else lines.append('  (none)')
    block('STANDING  approved; keep to these', every(tree, 'approved'),
          lambda d, owner: f"  [{d['anchor']}]  {d['title']}" + ('' if owner == tree['id'] else f'   · from {owner}'))
    block('NOT STAMPED  recommended, never approved; acting on one is a judgement call', every(tree, 'unstamped'),
          lambda d, owner: f"  [{d['anchor']}]  {d['title']}" + ('' if owner == tree['id'] else f'   · from {owner}'))
    mismatched = every(tree, 'mismatched')
    if mismatched:
        block('MISMATCHED  the page shows something else under these calls\' names; neither stamp nor verdict counts', mismatched,
              lambda d, owner: f"  [{d['anchor']}]  {d['title']}   · {d['problem']}" + ('' if owner == tree['id'] else f'   · on {owner}'))
    restamp = every(tree, 'restamp')
    if restamp:
        block('STAMPED ON ANOTHER VERSION  the wording changed since; ask again before relying on these', restamp,
              lambda d, owner: f"  [{d['anchor']}]  {d['title']}" + ('' if owner == tree['id'] else f'   · from {owner}'))
    overturned = every(tree, 'overturned')
    if overturned:
        block('OVERTURNED  the person said no; do not build on these', overturned,
              lambda d, owner: f"  [{d['anchor']}]  {d['title']}" + (f"   · \"{d['note']}\"" if d.get('note') else '') +
              ('' if owner == tree['id'] else f'   · from {owner}'))
    block('NOTES  the reader\'s words on these briefs; answer each before relying on the passage', every(tree, 'notes'),
          lambda n, owner: f"  [{n['anchor']}]  \"{n['text']}\"" + ('' if owner == tree['id'] else f'   · on {owner}'))
    if tree['ledger'] is None:
        lines.append('')
        lines.append('CALLS  ' + ('no ledger named: pass --calls, or name one in the brief' if tree['ledger_state'] == 'none named'
                                  else f"the ledger {tree['ledger_path']} is missing"))
    # A parent's calls are listed even when this brief names no ledger of its own.
    block('OPEN CALLS  carrying on as if the person concurs', every(tree, 'open_calls'),
          lambda c, owner: f"  [{c['id']}]  {c['title']}   · uncertainty {c['judgement']['uncertainty']}, taste {c['judgement']['taste']}"
          + ('' if owner == tree['id'] else f'   · from {owner}'))
    block('ASKED  stopped, waiting on the person', every(tree, 'asked_calls'),
          lambda c, owner: f"  [{c['id']}]  {c['title']}   · {'; '.join(c['reasons'])}" + ('' if owner == tree['id'] else f'   · from {owner}'))
    if tree['ledger'] and tree['ledger']['stale']:
        block('STALE  answered, but the evidence moved since', [(c,) for c in tree['ledger']['stale']],
              lambda c: f"  [{c['id']}]  {'; '.join(c['needs'])}")
    lines.append('')
    lines.append('SETTLED  ' + tree['settled']['state'])
    lines.extend('  · ' + why for why in tree['settled']['why'])
    return '\n'.join(lines) + '\n'


QUESTIONS = {
    'summary': 'what happened, what matters, and what follows?',
    'engineering': 'how does this work, why was it shaped this way, and what establishes that it works?',
    'decision': 'what choice is mine, what do you recommend, and what follows from choosing it?',
}
HTML_SCAFFOLD = '''<!doctype html>
<html lang="en"><head><title>{title}</title><!-- briefing: {meta} --><!--BRIEF:head-->
<style>
/* This brief's own figures and notes. The house look arrives through BRIEF:head. */
svg{display:block;width:100%;height:auto}.small{font-size:13px;color:var(--dim)}
</style></head><body>
<!--brief-content-->
<header class="top"><div class="wrap"><div class="eyebrow">Briefing · {purpose} · HTML</div><h1>{title}</h1><p class="lede">TODO(brief): two sentences on why this brief exists, for this reader.</p><nav class="sections"></nav></div></header>
<main class="wrap">
<!-- The reader must be able to answer: {question}
     Each section answers the question the one before it raised. Give every h2 and figure an id equal to its
     data-brief-anchor. Rename these ids now; once anyone has commented, keep an id when its heading changes. -->
<section class="pane"><h2 id="sec-finding" data-brief-anchor="sec-finding"><span class="n">01</span> TODO(brief): the first answer</h2>
<p>TODO(brief): the passage. Name a mechanism before using its identifiers, and link evidence where a claim needs it.</p>
<figure id="fig-mechanism" data-brief-anchor="fig-mechanism"><span class="lbl">01 · TODO(brief): what this drawing shows</span>
<svg viewBox="0 0 640 160" role="img" aria-label="TODO(brief): what the drawing shows"><rect class="box struct" x="20" y="40" width="200" height="80" rx="10"/><text class="ink" x="120" y="86" text-anchor="middle" font-size="16">TODO(brief)</text><path class="wire" d="M220 80 H420" fill="none"/><rect class="box unk" x="420" y="40" width="200" height="80" rx="10"/><text class="ink" x="520" y="86" text-anchor="middle" font-size="16">unknown, labelled</text></svg>
<figcaption><b>TODO(brief): a claim the reader can argue with.</b></figcaption><p class="small">TODO(brief): drawn from the code, a measurement, a proposal or an estimate.</p></figure></section>
<section class="pane"><h2 id="sec-next" data-brief-anchor="sec-next"><span class="n">02</span> TODO(brief): what follows</h2>
<p>TODO(brief): the consequence, the open question or the ask.</p></section>
<footer><p>Primary artifact: this brief. Evidence notes, feedback maps and source are supporting links.</p></footer></main>
<!--/brief-content-->
<!--BRIEF:notes--></body></html>
'''
MARKDOWN_SCAFFOLD = '''<!-- briefing: {meta} -->

# {title}

TODO(brief): two sentences on why this brief exists, for this reader.

<!-- The reader must be able to answer: {question}
     Each section answers the question the one before it raised. Rename these anchors now;
     once anyone has commented, keep an anchor when its heading changes. -->

<a id="sec-finding"></a>
## TODO(brief): the first answer

TODO(brief): the passage. Name a mechanism before using its identifiers, and link evidence where a claim needs it.

<a id="sec-next"></a>
## TODO(brief): what follows

TODO(brief): the consequence, the open question or the ask.
'''


def scaffold(path, identity, purpose, title=None, derives_from=(), calls=None):
    """Write a starting source with every required marker; never overwrite."""
    extras = {}
    if derives_from:
        missing = [str(p) for p in derives_from if not Path(p).is_file()]
        if missing:
            raise Refusal('--derives-from names briefs that are not there: ' + ', '.join(missing))
        extras['derives_from'] = [os.path.relpath(Path(p).resolve(), path.resolve().parent) for p in derives_from]
    if calls:
        if not str(calls).endswith('.calls.json'):
            raise Refusal('--calls names a NAME.calls.json ledger')
        extras['calls'] = os.path.relpath(Path(calls).resolve(), path.resolve().parent)
    if not SAFE_ID.fullmatch(identity):
        raise Refusal('use a stable lowercase brief id, such as cache-eviction-summary')
    if not purpose.strip():
        raise Refusal('name the purpose: summary, engineering, decision, or another outcome stated plainly')
    title = title or identity.replace('-', ' ').capitalize()
    question = QUESTIONS.get(purpose, f'what outcome does this {purpose} brief give its reader?')
    if path.suffix == '.md':
        meta = json.dumps({'id': identity, 'medium': 'markdown', 'purpose': purpose, **extras}, ensure_ascii=False)
        body = MARKDOWN_SCAFFOLD.replace('{meta}', meta).replace('{title}', title)
        steps = [f'python3 {Path(__file__).resolve()} check {path}']
    elif path.suffix == '.html':
        meta = json.dumps({'id': identity, 'purpose': purpose, **extras}, ensure_ascii=False).replace('--', '\\u002d\\u002d')
        body = HTML_SCAFFOLD.replace('{meta}', meta).replace('{title}', html_escape(title)).replace('{purpose}', html_escape(purpose))
        output = path.with_name(path.name.removesuffix('.src.html') + '.html') if path.name.endswith('.src.html') else path.with_suffix('.out.html')
        steps = [f'python3 {Path(__file__).resolve()} html {path} {output}',
                 f'python3 {Path(__file__).resolve()} check {output}']
    else:
        raise Refusal('name the source .md for Markdown or .src.html for HTML')
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open('x', encoding='utf-8') as stream:
            stream.write(body.replace('{question}', html_escape(question) if path.suffix == '.html' else question))
    except FileExistsError as exc:
        raise Refusal(f'{path} exists; new never overwrites a file') from exc
    return {'created': str(path), 'medium': 'markdown' if path.suffix == '.md' else 'html', 'purpose': purpose,
            'unwritten': f'replace every {UNWRITTEN}; check refuses a brief that still has one', 'next': steps}


def find_chromium():
    named = os.environ.get('CHROMIUM')
    if named:
        return named if shutil.which(named) or Path(named).is_file() else None
    for name in ('chromium', 'chromium-browser', 'google-chrome', 'google-chrome-stable', 'chrome'):
        found = shutil.which(name)
        if found:
            return found
    mac = Path('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome')
    return str(mac) if mac.is_file() else None


def doctor():
    """Say which parts of the skill work on this machine, and why the rest do not."""
    report = {'python': sys.version.split()[0],
              'markdown': {'ready': True, 'why': 'new, check, feedback and resolve need only Python 3.9+'}}
    missing = kit_missing()
    report['html_build'] = {'ready': not missing, 'why': 'decision-brief and fancy-pants-brief are installed beside this skill'
                            if not missing else 'missing ' + ', '.join(missing)}
    if missing:
        report['wallpaper'] = {'ready': None, 'why': 'unknown until the decision-brief skill is installed'}
    else:
        listed = subprocess.run([sys.executable, str(KIT / 'scripts/brief-wall'), '--list'], capture_output=True, text=True)
        themes = listed.returncode == 0 and bool(listed.stdout.strip())
        vips = shutil.which('vips') is not None
        report['wallpaper'] = {'ready': themes and vips, 'why': 'Omarchy themes and vips found' if themes and vips else
                               ('no Omarchy themes; ' if not themes else '') + ('vips not on PATH; ' if not vips else '') +
                               'build with --no-wall for the stylesheet\'s own dark ground'}
    gate = LOOK / 'scripts/fancy-pants'
    report['glass_gate'] = {'ready': gate.is_file(), 'why': str(gate) if gate.is_file() else 'fancy-pants-brief is not installed beside this skill'}
    node = shutil.which('node')
    major = None
    if node:
        version = subprocess.run([node, '--version'], capture_output=True, text=True).stdout.strip()
        found = re.match(r'v(\d+)', version)
        major = int(found[1]) if found else None
    chromium = find_chromium()
    ready = bool(node and major and major >= 22 and chromium)
    report['browser_check'] = {'ready': ready, 'why': f'node {major} and {chromium}' if ready else
                               ('node not on PATH; ' if not node else '') +
                               (f'node {major} lacks a built-in WebSocket (needs 22+); ' if node and (major or 0) < 22 else '') +
                               ('no Chromium or Chrome found (set CHROMIUM=/path)' if not chromium else '')}
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    p = commands.add_parser('new', help='write a starting .md or .src.html source; never overwrites')
    p.add_argument('source', type=Path)
    p.add_argument('--id', required=True)
    p.add_argument('--purpose', required=True)
    p.add_argument('--title')
    p.add_argument('--derives-from', type=Path, action='append', default=[], metavar='BRIEF',
                   help='a brief this one builds on; orient reads what it approved too (repeatable)')
    p.add_argument('--calls', type=Path, metavar='LEDGER', help='the NAME.calls.json ledger this brief shows')
    p = commands.add_parser('html', help='assemble a .src.html source into a self-contained, commentable page')
    p.add_argument('source', type=Path)
    p.add_argument('output', type=Path)
    p.add_argument('--id', help='defaults to the source\'s <!-- briefing: --> comment')
    p.add_argument('--purpose', help='defaults to the source\'s <!-- briefing: --> comment')
    p.add_argument('--no-wall', action='store_true', help='build on the stylesheet\'s own dark ground, without an Omarchy wallpaper')
    p.add_argument('--discard-output-edits', action='store_true', help='rebuild over edits made to the page itself, keeping them in a .bak')
    helps = {'check': 'validate anchors, scaffold leftovers and every local link and fragment',
             'feedback': 'export embedded HTML notes, or a blank Markdown map, as a format-1 envelope',
             'resolve': 'place a returned map (JSON, Copy-map text, or - for stdin) against the current document'}
    for name in ('check', 'feedback', 'resolve'):
        p = commands.add_parser(name, help=helps[name])
        p.add_argument('document', type=Path)
        if name == 'resolve':
            p.add_argument('map', type=Path)
    commands.add_parser('doctor', help='report which commands work on this machine, and why the rest do not')
    p = commands.add_parser('orient', help='print the standing orders: what is approved, what is not, notes and open calls')
    p.add_argument('document', type=Path)
    p.add_argument('--calls', type=Path, metavar='LEDGER', help='defaults to the ledger the brief names')
    p.add_argument('--json', action='store_true')
    p = commands.add_parser('call', help='record a decision nobody made; exit 3 means stop and ask before acting')
    p.add_argument('ledger', type=Path)
    p.add_argument('--title', required=True, help='the call in plain words')
    p.add_argument('--chose', required=True)
    p.add_argument('--because', required=True)
    p.add_argument('--uncertainty', required=True, choices=LEVELS, help=f'read in the slot {SLOT}')
    p.add_argument('--taste', required=True, choices=LEVELS, help='how much this is the person\'s own taste')
    p.add_argument('--posture', choices=POSTURES, help='from the project\'s instructions; undeclared is standard')
    p.add_argument('--context')
    p.add_argument('--passed-over', action='append', default=[], metavar='OPTION')
    p.add_argument('--undo', help='what undoing it would take')
    p.add_argument('--door', choices=('two-way', 'one-way'))
    p.add_argument('--evidence', type=Path, action='append', default=[], metavar='FILE')
    p.add_argument('--conflicts', action='append', default=[], metavar='ANCHOR',
                   help='an approved decision this would contradict; always stops')
    p.add_argument('--under', type=Path, metavar='BRIEF', help='the approved brief the work runs under')
    p.add_argument('--id')
    p.add_argument('--filled-by', default='agent', help='who read the slot')
    p = commands.add_parser('calls', help='list calls and whether the work has settled (exit 2: not yet); --html renders them')
    p.add_argument('ledger', type=Path)
    p.add_argument('--html', action='store_true', help='print the brief section of open and asked calls')
    p.add_argument('--all', action='store_true', help='with --html, include answered calls')
    p.add_argument('--for', dest='for_source', type=Path, metavar='PAGE', help='with --html, the built page that will show the rows; links are made relative to it')
    p.add_argument('--number', help='with --html, the section number')
    p.add_argument('--sync', type=Path, metavar='BRIEF', help='take the CONCUR stamps saved in this built brief')
    p = commands.add_parser('verdict', help='record the person\'s answer to a call')
    p.add_argument('ledger', type=Path)
    p.add_argument('call_id')
    p.add_argument('verdict', choices=VERDICTS)
    p.add_argument('--note')
    p.add_argument('--follow-up', metavar='URL')
    p.add_argument('--superseded-by', metavar='CALL')
    args = parser.parse_args()
    try:
        if args.command == 'orient':
            tree = orient(args.document, args.calls)
            if args.json:
                print(json.dumps(tree, ensure_ascii=False, indent=2))
            else:
                sys.stdout.write(orient_text(tree))
            return 0
        if args.command == 'call':
            result = record_call(args.ledger, args.title, args.chose, args.because, args.uncertainty, args.taste,
                                 args.posture, args.context, args.passed_over, args.undo, args.door, args.evidence,
                                 args.conflicts, args.under, args.id, args.filled_by)
            print(json.dumps(result, ensure_ascii=False, indent=2))
            return 3 if result['outcome'] == 'stop' else 0
        if args.command == 'calls':
            if args.html:
                sys.stdout.write(render_calls(args.ledger, args.for_source, args.number, args.all))
                return 0
            result = sync_calls(args.ledger, args.sync) if args.sync else calls_report(args.ledger)
            print(json.dumps(result, ensure_ascii=False, indent=2))
            return 0 if result['settled'] else 2
        if args.command == 'verdict':
            result = record_verdict(args.ledger, args.call_id, args.verdict, args.note, args.follow_up, args.superseded_by)
            print(json.dumps(result, ensure_ascii=False, indent=2))
            return 0
        if args.command == 'new':
            result = scaffold(args.source, args.id, args.purpose, args.title, args.derives_from, args.calls)
        elif args.command == 'html':
            result = build(args.source, args.output, args.id, args.purpose, args.no_wall, args.discard_output_edits)
        elif args.command == 'check':
            result = check(args.document)
        elif args.command == 'feedback':
            result = feedback(args.document)
        elif args.command == 'doctor':
            result = doctor()
        else:
            result = resolve(args.document, args.map)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 2 if args.command == 'resolve' and any(c['status'] != 'resolved' for c in result['comments']) else 0
    except (Refusal, OSError, ValueError, subprocess.CalledProcessError) as exc:
        print('brief: ' + str(exc), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
