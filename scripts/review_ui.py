"""Bounded, isolated self-captures and optional native lifecycle diagnostics. No desktop automation."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def run(executable, args, timeout):
    startup = None
    if sys.platform == "win32":
        startup = subprocess.STARTUPINFO()
        startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
        startup.wShowWindow = subprocess.SW_HIDE
    result = subprocess.run([str(executable), *map(str, args)], cwd=ROOT,
                            timeout=timeout, capture_output=True, text=True,
                            startupinfo=startup)
    if result.returncode:
        raise RuntimeError(f"Application exited with {result.returncode}: {result.stderr}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=ROOT / "target/release/PhotoEnhancer.exe")
    parser.add_argument("--diagnose", action="store_true")
    args = parser.parse_args()
    executable = args.exe.resolve(strict=True)
    folder = ROOT / ".test-output/ui-review"
    folder.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="run-", dir=folder))
    fixture = ROOT / ".test-output/gui-fixtures/harbor.png"
    if not fixture.is_file():
        raise RuntimeError("Generate fixtures first: python scripts/generate_gui_fixtures.py")
    if args.diagnose:
        report = output / "runtime.json"
        run(executable, ["--diagnose", report, "--input", fixture], 25)
        evidence = json.loads(report.read_text())
        print(json.dumps(evidence, indent=2), flush=True)
        input_events = {"pointer_move", "pointer_boundary", "pointer_button", "wheel", "key"}
        if any(any(count and name in input_events for name, count in sample.get("events", {}).items())
               for sample in evidence["samples"]):
            raise RuntimeError("The idle sample received desktop input; it cannot certify settled idle behavior")
        if any(not sample["cpu_available"] or sample["window_visible"] != (sample["state"] != "hidden")
               or sample["layout_frames"] or sample["raster_frames"] or sample["presentations"]
               or sample["cpu_percent_one_core"] > 5 for sample in evidence["samples"]):
            raise RuntimeError("Runtime gate requires investigation; see the diagnostic report")
        if evidence["graphics_modules"]:
            raise RuntimeError("Unexpected graphics API modules loaded")
    else:
        from PIL import Image
        for name, screen, width, height, scale in [
            ("workspace", "workspace", 1280, 900, 1),
            ("settings", "settings", 1280, 900, 1),
            ("workspace-compact", "workspace", 920, 640, 1),
            ("settings-scaled", "settings", 920, 640, 1.25),
            ("empty", "empty", 920, 640, 1),
            ("error", "error", 920, 640, 1),
        ]:
            target = output / f"{name}.png"
            selected_input = fixture.with_name("invalid.png") if screen == "error" else fixture
            run(executable, ["--capture", target, "--screen", screen, "--input", selected_input,
                             "--width", width, "--height", height, "--scale", scale], 15)
            with Image.open(target) as image:
                image.load()
                assert image.size == (round(width * scale), round(height * scale))
                assert len(image.getcolors(image.width * image.height)) > 20
            print(target, flush=True)
    print(f"Evidence directory: {output}", flush=True)


if __name__ == "__main__":
    main()
