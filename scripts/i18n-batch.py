#!/usr/bin/env python3
"""Validate and insert a complete translation batch into an existing Rust table.

Each entry has key, context, en, and translations (all nine language codes).
Values are strings or objects with all four Plural forms. Nothing is written
until the entire batch validates. --check validates without changing Rust.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import re
import tempfile
import os

LANGUAGES = ('en', 'es', 'de', 'fr', 'ru', 'zh', 'ja', 'ko', 'hi')
FORMS = ('one', 'few', 'many', 'other')
PLACEHOLDER = re.compile(r'(?<!\{)\{([a-zA-Z_][a-zA-Z_0-9]*)\}(?!\})')
KEYCAP = re.compile(r'\b(?:(?:Ctrl|Alt|Shift|Super)\+)+(?:[\w=\[\]/−-]+|[↑↓←→]+)(?:\s*…\s*[A-Za-z0-9]+)?|\b(?:Ctrl|Alt|Shift|Super|PgUp|PgDn|Esc|Enter|Backspace|Tab|Space)\b')


def forms(value):
    if isinstance(value, str):
        return {'other': value}
    if isinstance(value, dict) and set(value) == set(FORMS) and all(isinstance(v, str) for v in value.values()):
        return value
    raise ValueError('expected a string or exactly one/few/many/other')


def validate(batch):
    entries = batch['entries']
    seen = set()
    for entry in entries:
        key = entry['key']
        if not re.fullmatch(r'[a-z][a-z_0-9]*', key) or key in seen:
            raise ValueError(f'invalid or duplicate key: {key}')
        seen.add(key)
        if not entry.get('context', '').strip():
            raise ValueError(f'{key}: context required')
        exceptions = entry.get('same_as_english', [])
        if not set(exceptions) <= set(LANGUAGES) - {'en'} or (exceptions and not entry.get('cognate_reason', '').strip()):
            raise ValueError(f'{key}: cognate exceptions need valid language codes and a reason')
        source = forms(entry['en'])
        translations = entry['translations']
        if set(translations) != set(LANGUAGES):
            raise ValueError(f'{key}: expected all nine languages')
        if translations['en'] != entry['en']:
            raise ValueError(f'{key}: English source changed')
        for code, value in translations.items():
            target = forms(value)
            if set(target) != set(source):
                raise ValueError(f'{key}/{code}: plural forms changed')
            for form, text in target.items():
                original = source[form]
                if not text.strip():
                    raise ValueError(f'{key}/{code}/{form}: empty translation')
                if Counter(PLACEHOLDER.findall(text)) != Counter(PLACEHOLDER.findall(original)):
                    raise ValueError(f'{key}/{code}/{form}: placeholders changed')
                if Counter(KEYCAP.findall(text)) != Counter(KEYCAP.findall(original)):
                    raise ValueError(f'{key}/{code}/{form}: keycaps changed')
                if code != 'en' and text == original and re.search(r'[A-Za-z]{2}', text) and code not in entry.get('same_as_english', []):
                    raise ValueError(f'{key}/{code}/{form}: English fallback needs an explicit cognate exception')
    return entries


def literal(text):
    escapes = {'"': '\\"', '\\': '\\\\', '\n': '\\n', '\r': '\\r', '\t': '\\t'}
    return '"' + ''.join(escapes.get(c, f'\\u{{{ord(c):x}}}' if ord(c) < 32 else c) for c in text) + '"'


def rust(value):
    if isinstance(value, str):
        return literal(value)
    return 'Plural { ' + ', '.join(f'{form}: {literal(value[form])}' for form in FORMS) + ' }'


def insert(source, batch):
    entries = validate(batch)
    struct = batch.get('struct', 'Parts')
    marker = f'pub struct {struct} {{'
    if source.count(marker) != 1:
        raise ValueError(f'expected one {struct} struct')
    for entry in entries:
        if re.search(r'\b' + re.escape(entry['key']) + r'\s*:', source):
            raise ValueError(f"field already exists: {entry['key']}")
    declarations = '\n'.join('    /// ' + ' '.join(e['context'].split()) + '\n    pub ' + e['key'] + ": " + ("&'static str" if isinstance(e['en'], str) else 'Plural') + ',' for e in entries)
    source = source.replace(marker, marker + '\n' + declarations, 1)
    for code in LANGUAGES:
        pattern = rf'(pub const {code.upper()}: {struct} = {struct} \{{)'
        if len(re.findall(pattern, source)) != 1:
            raise ValueError(f'expected one {code} table')
        rows = '\n'.join(f"    {e['key']}: {rust(e['translations'][code])}," for e in entries)
        source = re.sub(pattern, lambda m: m[1] + '\n' + rows, source, count=1)
    return source


def verify(source, batch):
    """Verify a committed batch against all installed tables after rustfmt."""
    entries = validate(batch)
    struct = batch.get('struct', 'Parts')
    tokens = lambda value: re.findall(r'"(?:[^"\\]|\\.)*"|\S', value)
    for code in LANGUAGES:
        table = re.search(rf'^pub const {code.upper()}: {struct} = {struct} \{{(.*?)^\}};', source, re.M | re.S)
        if table is None:
            raise ValueError(f'missing {code} table')
        for entry in entries:
            field = re.search(r'^    ' + re.escape(entry['key']) + r':\s*("(?:[^"\\]|\\.)*"|Plural\s*\{.*?\}),', table[1], re.M | re.S)
            if field is None or tokens(field[1]) != tokens(rust(entry['translations'][code])):
                raise ValueError(f"{entry['key']}/{code}: installed value differs from batch")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('batch', type=Path)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--verify', action='store_true', help='verify an already inserted batch, including every installed translation')
    args = parser.parse_args()
    batch = json.loads(args.batch.read_text())
    entries = validate(batch)
    relative = Path(batch.get('target', 'app/src/lang/parts.rs'))
    if relative.is_absolute() or '..' in relative.parts or relative.parent != Path('app/src/lang'):
        raise ValueError('target must be a table under app/src/lang')
    path = args.root / relative
    if args.verify:
        verify(path.read_text(), batch)
        print(f'Verified {len(entries)} installed fields × 9 languages: {relative}')
        return
    result = insert(path.read_text(), batch)
    if not args.check:
        # Atomic replacement; validation cannot leave a half-written pack.
        with tempfile.NamedTemporaryFile('w', dir=path.parent, delete=False) as out:
            name = out.name
            try:
                out.write(result)
                out.flush()
                os.fsync(out.fileno())
            except BaseException:
                os.unlink(name)
                raise
        os.chmod(name, path.stat().st_mode)
        os.replace(name, path)
    print(f"{'Validated' if args.check else 'Inserted'} {len(entries)} fields × 9 languages: {relative}")


if __name__ == '__main__':
    main()
