"""Enforce the active Windows UI dependency graph without executing an EXE."""
from pathlib import Path
import argparse
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--offline", action="store_true", help="Use only cached dependencies")
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
command = [
    "cargo", "tree", "--locked", "--target", "x86_64-pc-windows-msvc",
    "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
]
if args.offline:
    command.append("--offline")
tree = subprocess.check_output(command, cwd=root, text=True)
names = {line.split()[0] for line in tree.splitlines() if line.strip()}
forbidden = {"eframe", "egui_glow", "egui-wgpu", "glow", "glutin", "glutin-winit",
             "gl_generator", "wgpu", "wgpu-core", "wgpu-hal", "ash", "vulkano"}
assert not (names & forbidden), f"GPU/UI dependency regression: {sorted(names & forbidden)}"
required = {"egui", "egui-winit", "egui_software_backend", "softbuffer", "winit"}
assert required <= names, f"Missing software backend dependencies: {sorted(required - names)}"
print("Windows dependency graph: CPU rasterizer and software presenter; no OpenGL/GPU backend.")
