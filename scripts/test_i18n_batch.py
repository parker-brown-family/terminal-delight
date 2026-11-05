import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('batch', Path(__file__).with_name('i18n-batch.py'))
batch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(batch)


class BatchTests(unittest.TestCase):
    def setUp(self):
        self.entry = {'key': 'example', 'context': 'A hint beside a button', 'en': 'Press Ctrl+T for {n}',
                      'translations': {l: f'{l} Ctrl+T {{n}}' for l in batch.LANGUAGES}}
        self.entry['translations']['en'] = self.entry['en']
        self.data = {'entries': [self.entry]}
        self.source = 'pub struct Parts {\n}\n' + '\n'.join(f'pub const {l.upper()}: Parts = Parts {{\n}};' for l in batch.LANGUAGES)

    def test_inserts_every_table_and_refuses_second_insertion(self):
        result = batch.insert(self.source, self.data)
        self.assertEqual(result.count('example:'), 10)
        with self.assertRaisesRegex(ValueError, 'already exists'):
            batch.insert(result, self.data)

    def test_rejects_missing_language_placeholder_keycap_and_fallback(self):
        for value, message in [('es Ctrl+T', 'placeholders'), ('es {n}', 'keycaps'), (self.entry['en'], 'English fallback')]:
            data = copy.deepcopy(self.data)
            data['entries'][0]['translations']['es'] = value
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, message):
                batch.validate(data)
        del self.entry['translations']['es']
        with self.assertRaisesRegex(ValueError, 'nine languages'):
            batch.validate(self.data)

    def test_rejects_a_lost_shortcut_range_endpoint(self):
        self.entry['en'] = 'Press Alt+1 … 4 for {n}'
        self.entry['translations'] = {l: f'{l} Alt+1 … 4 {{n}}' for l in batch.LANGUAGES}
        self.entry['translations']['en'] = self.entry['en']
        self.entry['translations']['es'] = 'es Alt+1 {n}'
        with self.assertRaisesRegex(ValueError, 'keycaps'):
            batch.validate(self.data)

    def test_verifies_installed_values_and_detects_drift(self):
        installed = batch.insert(self.source, self.data)
        batch.verify(installed, self.data)
        with self.assertRaisesRegex(ValueError, 'differs from batch'):
            batch.verify(installed.replace('es Ctrl+T {n}', 'es Ctrl+T changed {n}'), self.data)

    def test_preserves_control_characters_as_valid_rust_escapes(self):
        self.assertEqual(batch.literal('a\x08b\x0cc\x01'), '"a\\u{8}b\\u{c}c\\u{1}"')
        self.assertEqual(batch.literal(r'C:\bin\file'), '"C:\\\\bin\\\\file"')

    def test_rejects_lost_plural_and_duplicate_placeholders(self):
        self.entry['en'] = {f: '{n} things' for f in batch.FORMS}
        self.entry['translations'] = {l: {f: '{n} cosas' for f in batch.FORMS} for l in batch.LANGUAGES}
        self.entry['translations']['en'] = self.entry['en']
        batch.validate(self.data)
        del self.entry['translations']['ru']['few']
        with self.assertRaisesRegex(ValueError, 'one/few/many/other'):
            batch.validate(self.data)


if __name__ == '__main__':
    unittest.main()
