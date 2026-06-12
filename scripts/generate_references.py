#!/usr/bin/env python3

import sys
import subprocess
import numpy as np
import imagecodecs
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
TESTS_DIR = SCRIPT_DIR.parent / "vexel" / "tests"
IMAGES_DIR = TESTS_DIR / "images"
REFERENCES_DIR = TESTS_DIR / "references"


def usage() -> None:
    print("Usage: generate_references.py [-h] [--verify] [folder | file]")
    print()
    print("Convert images to lossless AVIF reference files.")
    print()
    print("Arguments:")
    print("  folder    Convert only the specified subfolder of images/")
    print("  file      Convert a single file (path relative to images/ or absolute)")
    print("            If omitted, all subfolders are converted.")
    print()
    print("Options:")
    print("  -h        Show this help message and exit")
    print("  --verify  Report MSE between each source image and its reference AVIF")
    print()
    print("Output is written to references/<folder>/<name>.avif.")
    print("ICC profiles, EXIF, and other metadata are stripped during decode.")
    print("Existing files are skipped.")


def decode_pixels(file: Path) -> tuple[int, int, np.ndarray]:
    r2 = subprocess.run(
        ["convert", str(file), "-coalesce", "-format", "%w %h\n", "-identify", "null:"],
        capture_output=True,
    )
    if r2.returncode != 0:
        raise RuntimeError(r2.stderr.decode().strip().splitlines()[0])

    frame_dims = [tuple(map(int, line.split())) for line in r2.stdout.decode().strip().splitlines()]
    frames = len(frame_dims)

    r = subprocess.run(
        ["convert", "-strip", str(file), "-coalesce", "-depth", "8", "rgba:-"],
        capture_output=True,
    )
    if r.returncode != 0:
        raise RuntimeError(r.stderr.decode().strip().splitlines()[0])

    if frames == 1:
        w, h = frame_dims[0]
        pixels = np.frombuffer(r.stdout, dtype=np.uint8).reshape(h, w, 4)
        return w, h, pixels

    w, h = frame_dims[0]
    total_expected = w * h * 4 * frames
    if len(r.stdout) != total_expected:
        raise RuntimeError(
            f"raw pixel data size {len(r.stdout)} does not match expected {total_expected} for {frames} frames"
        )

    pixels = np.frombuffer(r.stdout, dtype=np.uint8).reshape(frames, h, w, 4)
    return w, h, pixels


def convert_file(file: Path) -> None:
    try:
        relative = file.relative_to(IMAGES_DIR)
    except ValueError:
        print(f"File is not inside {IMAGES_DIR}: {file}", file=sys.stderr)
        sys.exit(1)

    dst = REFERENCES_DIR / relative.parent
    dst.mkdir(parents=True, exist_ok=True)

    output = dst / (file.stem + ".avif")
    if output.exists():
        print(f"Skipping {file} (already exists)")
        return

    frames_label = ""
    print(f"Converting {file} -> {output}")

    try:
        w, h, pixels = decode_pixels(file)
    except RuntimeError as e:
        print(f"  ERROR decoding {file}: {e}", file=sys.stderr)
        return

    if pixels.ndim == 4:
        frames_label = f" ({pixels.shape[0]} frames)"

    encoded = imagecodecs.avif_encode(pixels, level=100, speed=6)
    output.write_bytes(encoded)
    print(f"  Done{frames_label}")


def verify_file(file: Path) -> tuple[float | None, str | None]:
    try:
        relative = file.relative_to(IMAGES_DIR)
    except ValueError:
        return None, f"File is not inside {IMAGES_DIR}: {file}"

    reference = REFERENCES_DIR / relative.parent / (file.stem + ".avif")
    if not reference.exists():
        return None, "no reference"

    try:
        w, h, src_pixels = decode_pixels(file)
    except RuntimeError as e:
        return None, str(e)

    try:
        ref_pixels = imagecodecs.avif_decode(reference.read_bytes())
    except Exception as e:
        return None, f"AVIF decode error: {e}"

    if src_pixels.shape[:-1] != ref_pixels.shape[:-1]:
        return None, f"shape mismatch: src={src_pixels.shape} ref={ref_pixels.shape}"

    c = min(src_pixels.shape[-1], ref_pixels.shape[-1])
    src_pixels = src_pixels[..., :c]
    ref_pixels = ref_pixels[..., :c]

    mse = float(np.mean((src_pixels.astype(np.int32) - ref_pixels.astype(np.int32)) ** 2))
    return mse, None


def verify_folder(folder: str) -> None:
    src = IMAGES_DIR / folder
    if not src.is_dir():
        print(f"Folder not found: {src}", file=sys.stderr)
        sys.exit(1)

    files = sorted(f for f in src.iterdir() if f.is_file())
    if not files:
        return

    max_name = max(len(f.name) for f in files)
    for file in files:
        mse, err = verify_file(file)
        if err:
            print(f"  {file.name:<{max_name}}  ERROR: {err}")
        else:
            print(f"  {file.name:<{max_name}}  MSE: {mse:.5f}")


def convert_folder(folder: str) -> None:
    src = IMAGES_DIR / folder
    dst = REFERENCES_DIR / folder

    if not src.is_dir():
        print(f"Folder not found: {src}", file=sys.stderr)
        sys.exit(1)

    dst.mkdir(parents=True, exist_ok=True)

    for file in sorted(src.iterdir()):
        if not file.is_file():
            continue
        convert_file(file)


if "-h" in sys.argv:
    usage()
    sys.exit(0)

args = [a for a in sys.argv[1:] if not a.startswith("-")]
verify = "--verify" in sys.argv

if verify:
    if args:
        arg = Path(args[0])
        if not arg.is_absolute():
            arg = IMAGES_DIR / arg
        if arg.is_file():
            mse, err = verify_file(arg)
            name = arg.name
            if err:
                print(f"{name}  ERROR: {err}")
            else:
                print(f"{name}  MSE: {mse:.5f}")
        else:
            print(f"=== {args[0]} ===")
            verify_folder(args[0])
    else:
        for folder in sorted(IMAGES_DIR.iterdir()):
            if folder.is_dir():
                print(f"=== {folder.name} ===")
                verify_folder(folder.name)
elif args:
    arg = Path(args[0])
    if not arg.is_absolute():
        arg = IMAGES_DIR / arg
    if arg.is_file():
        convert_file(arg)
    else:
        convert_folder(args[0])
else:
    for folder in sorted(IMAGES_DIR.iterdir()):
        if folder.is_dir():
            convert_folder(folder.name)
