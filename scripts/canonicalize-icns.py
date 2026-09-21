#!/usr/bin/env python3
"""Sort ICNS chunks so Tauri icon generation is byte-for-byte reproducible."""

import argparse
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("path", type=Path)
    return parser.parse_args()


def main() -> None:
    path = parse_args().path
    contents = path.read_bytes()
    if contents[:4] != b"icns" or len(contents) < 8:
        raise RuntimeError(f"Not an ICNS container: {path}")
    if int.from_bytes(contents[4:8], "big") != len(contents):
        raise RuntimeError(f"ICNS length header does not match file size: {path}")

    chunks: list[bytes] = []
    offset = 8
    while offset < len(contents):
        if offset + 8 > len(contents):
            raise RuntimeError(f"Truncated ICNS chunk header: {path}")
        length = int.from_bytes(contents[offset + 4 : offset + 8], "big")
        if length < 8 or offset + length > len(contents):
            raise RuntimeError(f"Invalid ICNS chunk length: {path}")
        chunks.append(contents[offset : offset + length])
        offset += length

    canonical = b"icns" + len(contents).to_bytes(4, "big") + b"".join(
        sorted(chunks, key=lambda chunk: chunk[:4])
    )
    path.write_bytes(canonical)


if __name__ == "__main__":
    main()
