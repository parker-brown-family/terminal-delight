import runpy
import tempfile
import unittest
from pathlib import Path


audit_draws = runpy.run_path(str(Path(__file__).with_name("window-bench.py")))["audit_draws"]


class FrameAuditTest(unittest.TestCase):
    def test_only_complete_records_can_establish_a_draw(self):
        with tempfile.TemporaryDirectory() as temp:
            log = Path(temp) / "window.log"
            good = "td-frame direct=true post=false shadows=0\n"
            partial = "td-frame direct=true post="
            log.write_text(good + partial)
            audit_draws(log, "terminimalist")
            log.write_text(partial)
            with self.assertRaisesRegex(RuntimeError, "audit missing"):
                audit_draws(log, "terminimalist")
            for wrong in ["td-frame direct=false post=true shadows=0", "td-frame direct=true post=false shadows=1"]:
                log.write_text(good + wrong + "\n" + partial)
                with self.assertRaisesRegex(RuntimeError, "optional draw"):
                    audit_draws(log, "terminimalist")
