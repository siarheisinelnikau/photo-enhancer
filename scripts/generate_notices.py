"""Collect the locked Windows dependency licenses from the local Cargo cache."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    command = ["cargo", "metadata", "--locked", "--format-version", "1",
               "--filter-platform", "x86_64-pc-windows-msvc"]
    if args.offline:
        command.append("--offline")
    metadata = json.loads(subprocess.check_output(command, cwd=ROOT, encoding="utf-8"))
    # cargo tree reports active features; metadata can also retain inactive optional
    # graph nodes needed for lock resolution (for example the unused wgpu backend).
    tree_command = ["cargo", "tree", "--locked", "--target", "x86_64-pc-windows-msvc",
                    "--edges", "normal,build", "--prefix", "none", "--format", "{p}"]
    if args.offline:
        tree_command.append("--offline")
    tree = subprocess.check_output(tree_command, cwd=ROOT, encoding="utf-8")
    active = {tuple(line.split()[:2]) for line in tree.splitlines()}
    project_version = next(p["version"] for p in metadata["packages"] if p["source"] is None)
    notices = [f"PhotoEnhancer {project_version} — third-party notices\n",
               "Generated from Cargo.lock for Windows x64. Includes build dependencies.\n",
               "\nPhotoEnhancer license\n\n", (ROOT / "LICENSE").read_text(encoding="utf-8")]
    missing = []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if (package["name"], "v" + package["version"]) not in active or package["source"] is None:
            continue
        directory = Path(package["manifest_path"]).parent
        notices.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\n"
                       f"License: {package['license']}\n"
                       f"Source: {package['repository'] or package['source']}\n")
        files = set()
        for pattern in ("LICENSE*", "LICENCE*", "COPYING*", "NOTICE*", "license*", "Copyright*", "COPYRIGHT*"):
            files.update(p for p in directory.glob(pattern) if p.is_file())
        if package["license_file"]:
            files.add(directory / package["license_file"])
        if package["name"] == "epaint_default_fonts":
            files.update((directory / "fonts").glob("*.txt"))
            notices.append("The font crate's Rust code uses the Apache-2.0 option; individual font licenses follow.\n")
            notices.append((ROOT / "docs/licenses/Apache-2.0.txt").read_text(encoding="utf-8"))
        # Some crates keep additional notices in a licenses directory (e.g. ring).
        for subdirectory in ("LICENSES", "licenses"):
            if (directory / subdirectory).is_dir():
                files.update(p for p in (directory / subdirectory).rglob("*") if p.is_file())
        if not files:
            license_id = package["license"]
            if license_id in ("MIT OR Apache-2.0", "Apache-2.0", "BSL-1.0"):
                choice = "Apache-2.0" if "Apache-2.0" in license_id else license_id
                notices.append(f"Using the {choice} option.\n")
                notices.append((ROOT / "docs" / "licenses" / (choice + ".txt")).read_text(encoding="utf-8"))
            else:
                missing.append(f"{package['name']} {package['version']}: {license_id}")
                notices.append("License text not shipped in this crate source archive.\n")
        for file in sorted(files):
            notices.append(f"\n--- {file.relative_to(directory).as_posix()} ---\n")
            notices.append(file.read_text(encoding="utf-8", errors="replace") + "\n")
    target = ROOT / "docs" / "THIRD_PARTY_NOTICES.txt"
    target.parent.mkdir(exist_ok=True)
    normalized = "\n".join(line.rstrip() for line in "".join(notices).splitlines()).rstrip() + "\n"
    target.write_text(normalized, encoding="utf-8")
    print(f"Wrote {target}; {len(active)-1} dependencies; {len(missing)} missing license texts")
    for entry in missing:
        print(entry)
    if missing:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
