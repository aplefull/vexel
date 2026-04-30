#!/usr/bin/env python3

import sys
import subprocess
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
IMAGES_DIR = SCRIPT_DIR / "images"
REFERENCES_DIR = SCRIPT_DIR / "references"


def convert_folder(folder: str) -> None:
    src = IMAGES_DIR / folder
    dst = REFERENCES_DIR / folder

    if not src.is_dir():
        print(f"Folder not found: {src}", file=sys.stderr)
        sys.exit(1)

    dst.mkdir(parents=True, exist_ok=True)

    for file in src.iterdir():
        if not file.is_file():
            continue
        output = dst / (file.stem + ".webp")
        if output.exists():
            print(f"Skipping {file} (already exists)")
            continue
        print(f"Converting {file} -> {output}")
        subprocess.run(["convert", str(file), "-define", "webp:lossless=true", str(output)], check=True)


if len(sys.argv) == 2:
    convert_folder(sys.argv[1])
else:
    for folder in sorted(IMAGES_DIR.iterdir()):
        if folder.is_dir():
            convert_folder(folder.name)
