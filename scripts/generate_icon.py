"""Build the original photo/mountain/sparkle icon from shared vector geometry."""
from pathlib import Path
import struct
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"


def render(size):
    scale = 4
    canvas = Image.new("RGBA", (512, 512))
    draw = ImageDraw.Draw(canvas)
    box = lambda xy: tuple(round(v * scale) for v in xy)
    draw.rounded_rectangle(box((3, 3, 125, 125)), radius=26 * scale, fill="#161e28")
    draw.rounded_rectangle(box((19, 28, 109, 105)), radius=12 * scale,
                           fill="#203d49", outline="#53c6b1", width=5 * scale)
    draw.polygon([box(p) for p in [(25, 94), (51, 58), (73, 83), (86, 68), (103, 94)]], fill="#53c6b1")
    draw.ellipse(box((76, 40, 92, 56)), fill="#edbe6a")
    draw.polygon([box(p) for p in [(106, 8), (110, 23), (125, 27), (110, 31),
                                 (106, 46), (102, 31), (87, 27), (102, 23)]], fill="#e7edf4")
    return canvas.resize((size, size), Image.Resampling.LANCZOS)


def main():
    ASSETS.mkdir(exist_ok=True)
    image = render(256)
    image.save(ASSETS / "app-icon.png")
    image.save(ASSETS / "app-icon.ico", sizes=[(n, n) for n in (16, 20, 24, 32, 40, 48, 64, 128, 256)])
    # Pillow writes zero planes for PNG entries; use the Windows resource value.
    ico = bytearray((ASSETS / "app-icon.ico").read_bytes())
    for index in range(struct.unpack_from("<H", ico, 4)[0]):
        struct.pack_into("<H", ico, 6 + index * 16 + 4, 1)
    (ASSETS / "app-icon.ico").write_bytes(ico)
    svg = '''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
<rect x="3" y="3" width="122" height="122" rx="26" fill="#161e28"/>
<rect x="19" y="28" width="90" height="77" rx="12" fill="#203d49" stroke="#53c6b1" stroke-width="5"/>
<path d="M25 94 51 58 73 83 86 68 103 94Z" fill="#53c6b1"/>
<circle cx="84" cy="48" r="8" fill="#edbe6a"/>
<path d="M106 8 110 23 125 27 110 31 106 46 102 31 87 27 102 23Z" fill="#e7edf4"/>
</svg>
'''
    (ASSETS / "app-icon.svg").write_text(svg, encoding="utf-8")
    print(ASSETS / "app-icon.ico")


if __name__ == "__main__":
    main()
