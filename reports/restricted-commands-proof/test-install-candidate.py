import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("installer", Path(__file__).with_name("install-candidate.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class CandidateSelection(unittest.TestCase):
    def test_preview_changes_nothing_and_apply_only_selects_checked_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            old, candidate, launcher = root / "td-1234567-before-review", root / "td-abcdef123456-restricted-review", root / "launcher"
            old.write_bytes(b"previous build")
            candidate.write_bytes(b"reviewed build")
            launcher.symlink_to(old)
            library = root / "library"
            digest = hashlib.sha256(candidate.read_bytes()).hexdigest()
            args = candidate, digest, old, launcher, library, root / "deploy.lock"
            preview = installer.select(*args)
            self.assertFalse(preview["applied"])
            self.assertEqual(launcher.resolve(), old)
            self.assertFalse(library.exists())
            selected = installer.select(*args, apply=True)
            self.assertTrue(selected["applied"])
            self.assertEqual(launcher.resolve(), library / candidate.name)
            self.assertEqual(launcher.read_bytes(), b"reviewed build")
            self.assertEqual(old.read_bytes(), b"previous build")
            reverted = installer.select(old, hashlib.sha256(old.read_bytes()).hexdigest(),
                library / candidate.name, launcher, library, root / "deploy.lock", True)
            self.assertTrue(reverted["applied"])
            self.assertEqual(launcher.read_bytes(), b"previous build")

    def test_wrong_digest_or_changed_launcher_is_refused_without_selection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            old, candidate, launcher = root / "td-1234567-before-review", root / "td-abcdef123456-restricted-review", root / "launcher"
            old.write_bytes(b"previous")
            candidate.write_bytes(b"candidate")
            launcher.symlink_to(old)
            digest = hashlib.sha256(candidate.read_bytes()).hexdigest()
            for wrong_hash, wrong_old in [("0" * 64, old), (digest, root / "different")]:
                with self.assertRaises(ValueError):
                    installer.select(candidate, wrong_hash, wrong_old, launcher, root / "library", root / "lock", True)
                self.assertEqual(launcher.resolve(), old)
                self.assertFalse((root / "library").exists())


if __name__ == "__main__":
    unittest.main()
