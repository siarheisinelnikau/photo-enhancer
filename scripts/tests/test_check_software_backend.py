"""Exercise dependency-audit modes without network or Cargo subprocesses."""
from contextlib import redirect_stdout
import io
from pathlib import Path
import runpy
import sys
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "check_software_backend.py"
SOFTWARE_GRAPH = "\n".join(
    f"{name} v1.0.0" for name in
    ("egui", "egui-winit", "egui_software_backend", "softbuffer", "winit")
)


class BackendAuditTests(unittest.TestCase):
    def invoke(self, arguments=(), graph=SOFTWARE_GRAPH):
        with patch.object(sys, "argv", [str(SCRIPT), *arguments]), patch(
            "subprocess.check_output", return_value=graph
        ) as cargo, redirect_stdout(io.StringIO()):
            runpy.run_path(str(SCRIPT), run_name="__main__")
        return cargo.call_args.args[0]

    def test_normal_ci_run_can_resolve_a_cold_cache_with_the_lockfile(self):
        command = self.invoke()
        self.assertIn("--locked", command)
        self.assertNotIn("--offline", command)
        self.assertEqual(command[command.index("--target") + 1], "x86_64-pc-windows-msvc")

    def test_offline_run_stays_explicitly_offline_and_locked(self):
        command = self.invoke(("--offline",))
        self.assertIn("--offline", command)
        self.assertIn("--locked", command)

    def test_gpu_dependency_is_still_rejected(self):
        with self.assertRaisesRegex(AssertionError, "GPU/UI dependency regression"):
            self.invoke(graph=SOFTWARE_GRAPH + "\nwgpu v1.0.0")

    def test_missing_software_renderer_is_still_rejected(self):
        with self.assertRaisesRegex(AssertionError, "Missing software backend dependencies"):
            self.invoke(graph="egui v1.0.0")


if __name__ == "__main__":
    unittest.main()
