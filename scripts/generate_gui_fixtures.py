"""Generate original, deterministic images for GUI checks; never reads photographs."""
from pathlib import Path
import math
import random

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / ".test-output" / "gui-fixtures"


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    width, height = 1600, 1000
    image = Image.new("RGB", (width, height))
    pixels = image.load()
    for y in range(height):
        t = min(y / 640, 1)
        color = tuple(round(a + (b - a) * t) for a, b in zip((35, 67, 107), (242, 166, 119)))
        for x in range(width):
            pixels[x, y] = color
    draw = ImageDraw.Draw(image)
    draw.ellipse((1070, 160, 1280, 370), fill=(255, 219, 153))
    draw.polygon([(0, 490), (235, 280), (410, 430), (645, 230), (900, 480), (1170, 355), (1600, 490), (1600, 1000), (0, 1000)], fill=(61, 85, 111))
    draw.polygon([(550, 340), (645, 230), (752, 337), (686, 313), (644, 334), (620, 300)], fill=(193, 204, 210))
    draw.polygon([(0, 575), (330, 415), (600, 570), (950, 455), (1280, 590), (1600, 465), (1600, 1000), (0, 1000)], fill=(31, 66, 82))
    draw.rectangle((0, 620, width, height), fill=(40, 103, 121))
    for y in range(640, height, 12):
        t = (y - 640) / 360
        half = int(80 + t * 150)
        center = 1175 + int(18 * math.sin(y * 0.08))
        draw.line((center - half, y, center + half, y), fill=(169, 160, 135), width=3)
    draw.polygon([(0, 790), (200, 710), (370, 760), (590, 1000), (0, 1000)], fill=(24, 49, 57))
    # An original lighthouse silhouette provides fine edges for sharpening checks.
    draw.polygon([(195, 735), (217, 452), (275, 452), (298, 735)], fill=(220, 209, 180))
    draw.polygon([(211, 505), (281, 505), (286, 565), (206, 565)], fill=(156, 75, 65))
    draw.rectangle((205, 428, 287, 465), fill=(32, 47, 60))
    draw.rectangle((218, 428, 274, 454), fill=(248, 207, 123))
    draw.polygon([(195, 427), (246, 390), (297, 427)], fill=(32, 47, 60))
    draw.rectangle((238, 659, 260, 717), fill=(37, 49, 56))
    for x in range(690, 960, 11):
        draw.line((x, 825, x, 900), fill=(76, 135, 143), width=2)
    draw.line((655, 914, 1008, 914), fill=(22, 62, 76), width=7)
    rng = random.Random(14)
    for y in range(height):
        for x in range(width):
            rgb = pixels[x, y]
            pixels[x, y] = tuple(max(0, min(255, round(c * 0.78) + rng.randint(-9, 9))) for c in rgb)
    image.save(OUTPUT / "harbor.png")
    image.resize((800, 500)).save(OUTPUT / "harbor-small.jpg", quality=92)
    alpha = Image.new("RGBA", (600, 400), (0, 0, 0, 0))
    art = ImageDraw.Draw(alpha)
    art.rounded_rectangle((40, 40, 560, 360), radius=75, fill=(52, 156, 160, 180))
    art.ellipse((185, 85, 415, 315), fill=(245, 184, 98, 255))
    alpha.save(OUTPUT / "alpha.png")
    (OUTPUT / "invalid.png").write_bytes(b"Generated invalid fixture; not a photograph.")
    print(OUTPUT)


if __name__ == "__main__":
    main()
