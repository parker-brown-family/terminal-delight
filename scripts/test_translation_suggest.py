import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('suggest', Path(__file__).with_name('translation-suggest.py'))
suggest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(suggest)


class DetectorTests(unittest.TestCase):
    def test_rustfmt_cannot_hide_a_control_word_or_status(self):
        source = 'sk.control(\n    "+",\n    "add",\n);\nstatus:\n    "Loading models…".into(),'
        self.assertEqual(suggest.continued_literals(source), {3: ['add'], 6: ['Loading models…']})
        self.assertEqual(suggest.continued_literals('Self::bezel_btn_s(\n &sk,\n "ADD PROJECT", false, scale)'), {3: ['ADD PROJECT']})

    def test_control_and_status_are_read_but_ids_and_placeholder_names_are_not(self):
        self.assertEqual(suggest.drawn_literals('sk.control("+", "add")'), ['add'])
        self.assertEqual(suggest.drawn_literals('status: "Loading models…".into(),'), ['Loading models…'])
        self.assertEqual(suggest.drawn_literals('.id("panel-id").child(fill(t, &[("query", &query)]))'), [])
        self.assertEqual(suggest.drawn_literals('SharedString::from("protocol key")'), [])

    def test_standalone_gpui_tests_raw_strings_and_adjacent_production(self):
        source = '''#[gpui::test]
fn fixture() {
    let raw = r##" } { "##;
    div().child("Test label");
}
fn production() { div().child("Real label"); }
'''
        spans = suggest.test_spans_source(source)
        self.assertEqual(spans, [(1, 5)])
        self.assertFalse(any(a <= 6 <= b for a, b in spans))

    def test_test_only_import_does_not_hide_next_production_function(self):
        self.assertEqual(suggest.test_spans_source('#[cfg(test)]\nuse fixture::Thing;\nfn draw() { div().child("Real label"); }'), [])


if __name__ == '__main__':
    unittest.main()
