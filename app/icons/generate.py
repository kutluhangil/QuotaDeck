#!/usr/bin/env python3
"""Generate the app icon set.

The mark is the product's own idea rather than any vendor's: a horizon line with bands of
capacity resting above it, the newest at the right. No third-party logo appears anywhere in
the app, which is both a trademark requirement and the design language (see blueprint 8.3).

Run from the repository root:

    python3 app/icons/generate.py

Requires nothing beyond the standard library. macOS `iconutil` turns the .iconset into the
.icns the bundle needs.
"""

from __future__ import annotations

import argparse
import pathlib
import struct
import tempfile
import zlib

HERE = pathlib.Path(__file__).parent

# Panel surface tokens, so the icon and the window are visibly the same object.
ABYSS = (0x0A, 0x0E, 0x17)
HULL = (0x13, 0x1A, 0x26)
GHOST = (0x2B, 0x3A, 0x52)
INK_TERTIARY = (0x55, 0x63, 0x7A)
INK_SECONDARY = (0x8A, 0x99, 0xB0)
INK_PRIMARY = (0xE6, 0xEB, 0xF4)

# Relative geometry, so every size is the same drawing rather than a separate design.
HORIZON_Y = 0.66
CORNER = 0.22
MARGIN = 0.14
# Usage enters at the right and drifts left until it falls off the edge, which is the whole
# point of a rolling window. Heights are irregular because real usage is bursty; an
# ascending run would read as a growth chart and tell the opposite story.
# The fourth field fades the band out towards the left edge, so the oldest usage reads as
# dissolving rather than as a short bar.
BANDS = [
    (0.00, 0.12, 0.44, GHOST, True),
    (0.18, 0.34, 0.80, INK_TERTIARY, True),
    (0.40, 0.52, 0.32, INK_TERTIARY, False),
    (0.58, 0.74, 0.62, INK_SECONDARY, False),
    (0.80, 1.00, 1.00, INK_PRIMARY, False),
]

# The capacity about to come back, shown below the line before it arrives.
RETURNING = (0.54, 1.00, 0.16)


def write_png(path: pathlib.Path, pixels: list[list[tuple[int, int, int, int]]]) -> None:
    height = len(pixels)
    width = len(pixels[0])
    raw = bytearray()
    for row in pixels:
        raw.append(0)  # filter type 0
        for r, g, b, a in row:
            raw += bytes((r, g, b, a))

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


def write_icns(path: pathlib.Path, iconset: pathlib.Path) -> None:
    """Write the documented modern ICNS chunks directly from their PNG payloads."""
    sources = [
        (b"icp4", "icon_16x16.png"),
        (b"icp5", "icon_32x32.png"),
        (b"icp6", "icon_32x32@2x.png"),
        (b"ic07", "icon_128x128.png"),
        (b"ic08", "icon_256x256.png"),
        (b"ic09", "icon_512x512.png"),
        (b"ic10", "icon_512x512@2x.png"),
        (b"ic11", "icon_16x16@2x.png"),
        (b"ic12", "icon_32x32@2x.png"),
        (b"ic13", "icon_128x128@2x.png"),
        (b"ic14", "icon_256x256@2x.png"),
    ]
    chunks = bytearray()
    for kind, name in sources:
        payload = (iconset / name).read_bytes()
        chunks += kind + struct.pack(">I", len(payload) + 8) + payload
    path.write_bytes(b"icns" + struct.pack(">I", len(chunks) + 8) + chunks)


def blend(under: tuple[int, int, int, int], colour: tuple[int, int, int], alpha: float):
    a = max(0.0, min(1.0, alpha))
    return (
        round(under[0] * (1 - a) + colour[0] * a),
        round(under[1] * (1 - a) + colour[1] * a),
        round(under[2] * (1 - a) + colour[2] * a),
        max(under[3], round(255 * a)),
    )


def coverage(distance: float, feather: float) -> float:
    """Analytic anti-aliasing: how much of a pixel falls inside an edge."""
    return max(0.0, min(1.0, 0.5 - distance / feather))


def rounded_rect_distance(x: float, y: float, size: float, radius: float) -> float:
    """Signed distance to a rounded square centred in a `size` box."""
    half = size / 2 - 0.5
    cx, cy = x - half, y - half
    qx = abs(cx) - (half - radius)
    qy = abs(cy) - (half - radius)
    if qx < 0 and qy < 0:
        return max(qx, qy) - radius
    dx, dy = max(qx, 0.0), max(qy, 0.0)
    return (dx * dx + dy * dy) ** 0.5 - radius


def draw(size: int) -> list[list[tuple[int, int, int, int]]]:
    radius = size * CORNER
    margin = size * MARGIN
    inner = size - 2 * margin
    horizon = margin + inner * HORIZON_Y
    # A hairline at 16px is a full pixel; scale it but never below one.
    rule = max(1.0, size / 32)
    feather = max(1.0, size / 256)

    pixels: list[list[tuple[int, int, int, int]]] = []
    for py in range(size):
        row: list[tuple[int, int, int, int]] = []
        for px in range(size):
            x, y = px + 0.5, py + 0.5
            pixel = (0, 0, 0, 0)

            body = coverage(rounded_rect_distance(x, y, size, radius), feather)
            if body <= 0:
                row.append(pixel)
                continue

            # Vertical wash keeps the tile from reading as flat fill.
            mix = y / size
            base = tuple(round(HULL[i] * (1 - mix) + ABYSS[i] * mix) for i in range(3))
            pixel = blend(pixel, base, body)

            for start, end, height, colour, fades in BANDS:
                left = margin + inner * start
                right = margin + inner * end - max(1.0, size / 64)
                if right <= left:
                    continue
                top = horizon - (horizon - margin) * height
                inside_x = min(x - left, right - x)
                inside_y = min(y - top, horizon - y)
                band = min(coverage(-inside_x, feather), coverage(-inside_y, feather))
                if fades:
                    # Ramp across the band and on past it, so the two oldest bands share one
                    # continuous dissolve towards the edge.
                    band *= min(1.0, max(0.0, (x - margin) / (inner * 0.42)))
                if band > 0:
                    pixel = blend(pixel, colour, band * body)

            # Returning capacity: a thin band under the line, ahead of the usage that will
            # release it.
            r_start, r_end, r_height = RETURNING
            r_left = margin + inner * r_start
            r_right = margin + inner * r_end
            r_bottom = horizon + rule / 2 + (size - margin - horizon) * r_height
            inside_x = min(x - r_left, r_right - x)
            inside_y = min(y - (horizon + rule / 2), r_bottom - y)
            ghost = min(coverage(-inside_x, feather), coverage(-inside_y, feather))
            if ghost > 0:
                pixel = blend(pixel, GHOST, ghost * body)

            # The horizon itself: the line usage falls off the edge of.
            line = coverage(abs(y - horizon) - rule / 2, feather)
            if line > 0:
                edge = min(coverage(-(x - margin), feather), coverage(-(size - margin - x), feather))
                pixel = blend(pixel, INK_PRIMARY, line * edge * body)

            row.append(pixel)
        pixels.append(row)
    return pixels


def generate(output: pathlib.Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    for size in (32, 128, 256, 512):
        write_png(output / f"{size}x{size}.png", draw(size))
    write_png(output / "icon.png", draw(512))

    iconset = output / "icon.iconset"
    iconset.mkdir(exist_ok=True)
    for size in (16, 32, 128, 256, 512):
        write_png(iconset / f"icon_{size}x{size}.png", draw(size))
        write_png(iconset / f"icon_{size}x{size}@2x.png", draw(size * 2))

    write_icns(output / "icon.icns", iconset)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="regenerate in a temporary directory and fail if tracked icons differ",
    )
    args = parser.parse_args()

    if not args.check:
        generate(HERE)
        print(f"wrote icons to {HERE}")
        return

    with tempfile.TemporaryDirectory(prefix="quotadeck-icons-") as temporary:
        generated = pathlib.Path(temporary)
        generate(generated)
        names = ["32x32.png", "128x128.png", "256x256.png", "512x512.png", "icon.png", "icon.icns"]
        changed = [name for name in names if (generated / name).read_bytes() != (HERE / name).read_bytes()]
        if changed:
            raise SystemExit(
                "generated icons are stale: " + ", ".join(changed) + "; run app/icons/generate.py"
            )
    print("tracked icons match the generator")


if __name__ == "__main__":
    main()
