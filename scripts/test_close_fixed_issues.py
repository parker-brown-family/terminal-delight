#!/usr/bin/env python3
"""The keyword reader behind close-fixed-issues.yml.

Mutation-checked: dropping the lookbehind lets a word that only ends in a
keyword ("prefixes #3") close #3, and dropping "fixed" from the pattern misses
"Fixed #7"; each fails a case.
"""
import runpy
import unittest
from pathlib import Path

fixed_numbers = runpy.run_path(str(Path(__file__).with_name("close_fixed_issues.py")))["fixed_numbers"]


class FixedNumbersTest(unittest.TestCase):
    def test_the_keywords_github_uses_each_name_an_issue(self):
        body = "Fixes #1508\nFixes #1084, Fixes #846\ncloses #12 and Resolved: #13\nFixed #7"
        self.assertEqual(fixed_numbers(body), [1508, 1084, 846, 12, 13, 7])

    def test_a_mention_is_not_a_fix(self):
        self.assertEqual(fixed_numbers("see #1491, related to #1503, after #1486"), [])
        self.assertEqual(fixed_numbers("the prefixes #3 and the unfixed #4 are words"), [])

    def test_another_repositorys_issue_is_left_alone(self):
        self.assertEqual(fixed_numbers("fixes parker-brown-family/agent-skills#84"), [])

    def test_each_number_once_and_no_body_is_no_fixes(self):
        self.assertEqual(fixed_numbers("Fixes #5. Fixes #5."), [5])
        self.assertEqual(fixed_numbers(None), [])
        self.assertEqual(fixed_numbers(""), [])


if __name__ == "__main__":
    unittest.main()
