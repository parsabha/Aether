"""Build NSIS sidebar/header BMPs and donation files from Aether branding."""

from __future__ import annotations

import json
import struct
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[1]
ICON_CANDIDATES = [
    ROOT / "src-tauri" / "icons" / "icon.png",
    ROOT / "app-icon.png",
]
OUT_DIR = ROOT / "src-tauri" / "installer"
SIDEBAR = (164, 314)
HEADER = (150, 57)

BG_TOP = (7, 8, 16)
BG_BOTTOM = (8, 22, 48)
ACCENT = (0, 120, 242)
GLOW = (12, 80, 180)
TEXT = (245, 245, 247)
SUBTLE = (140, 176, 214)


def load_icon() -> Image.Image:
    for path in ICON_CANDIDATES:
        if path.exists():
            return Image.open(path).convert("RGBA")
    raise SystemExit("No app icon found (expected src-tauri/icons/icon.png or app-icon.png)")


def font(size: int, bold: bool = True) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    names = (
        ["segoeuib.ttf", "seguisb.ttf", "segoeui.ttf"]
        if bold
        else ["segoeui.ttf", "seguisb.ttf"]
    )
    for name in names:
        candidate = Path(r"C:\Windows\Fonts") / name
        if candidate.exists():
            return ImageFont.truetype(str(candidate), size)
    return ImageFont.load_default()


def vertical_gradient(size: tuple[int, int]) -> Image.Image:
    w, h = size
    img = Image.new("RGB", size, BG_TOP)
    px = img.load()
    for y in range(h):
        t = y / max(h - 1, 1)
        t = t * t * (3 - 2 * t)
        r = int(BG_TOP[0] + (BG_BOTTOM[0] - BG_TOP[0]) * t)
        g = int(BG_TOP[1] + (BG_BOTTOM[1] - BG_TOP[1]) * t)
        b = int(BG_TOP[2] + (BG_BOTTOM[2] - BG_TOP[2]) * t)
        for x in range(w):
            px[x, y] = (r, g, b)
    return img


def add_glow(
    base: Image.Image,
    center: tuple[int, int],
    radius: int,
    color: tuple[int, int, int],
    strength: int = 140,
) -> None:
    overlay = Image.new("RGBA", base.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    x, y = center
    for i in range(6, 0, -1):
        r = int(radius * (i / 3.2))
        alpha = int(strength * (1 / i))
        draw.ellipse((x - r, y - r, x + r, y + r), fill=(*color, alpha))
    overlay = overlay.filter(ImageFilter.GaussianBlur(18))
    composed = Image.alpha_composite(base.convert("RGBA"), overlay)
    base.paste(composed.convert("RGB"))


def paste_icon(base: Image.Image, icon: Image.Image, box: tuple[int, int, int, int]) -> None:
    x0, y0, x1, y1 = box
    size = (x1 - x0, y1 - y0)
    fitted = icon.resize(size, Image.Resampling.LANCZOS)
    layer = Image.new("RGBA", base.size, (0, 0, 0, 0))
    layer.paste(fitted, (x0, y0), fitted)
    composed = Image.alpha_composite(base.convert("RGBA"), layer)
    base.paste(composed.convert("RGB"))


def save_bmp24(img: Image.Image, dest: Path) -> None:
    """Write an uncompressed bottom-up 24-bit BMP (what NSIS MUI expects)."""
    rgb = img.convert("RGB")
    w, h = rgb.size
    row_stride = (w * 3 + 3) & ~3
    pixel_bytes = bytearray(row_stride * h)
    src = rgb.tobytes()
    for y in range(h):
        src_y = h - 1 - y
        start = y * row_stride
        src_start = src_y * w * 3
        for x in range(w):
            r, g, b = src[src_start + x * 3 : src_start + x * 3 + 3]
            o = start + x * 3
            pixel_bytes[o] = b
            pixel_bytes[o + 1] = g
            pixel_bytes[o + 2] = r
    file_size = 54 + len(pixel_bytes)
    header = struct.pack(
        "<2sIHHIIiiHHIIiiII",
        b"BM",
        file_size,
        0,
        0,
        54,
        40,
        w,
        h,
        1,
        24,
        0,
        len(pixel_bytes),
        2835,
        2835,
        0,
        0,
    )
    dest.write_bytes(header + pixel_bytes)


def make_sidebar(icon: Image.Image) -> Image.Image:
    img = vertical_gradient(SIDEBAR)
    add_glow(img, (82, 118), 78, GLOW, 170)
    paste_icon(img, icon, (28, 52, 136, 160))
    draw = ImageDraw.Draw(img)
    title = font(22, bold=True)
    subtitle = font(10, bold=False)
    draw.line((36, 184, 128, 184), fill=ACCENT, width=1)
    draw.text((82, 210), "AETHER", font=title, fill=TEXT, anchor="mm")
    draw.text((82, 236), "Game launcher", font=subtitle, fill=SUBTLE, anchor="mm")
    draw.text((82, 292), "Windows", font=subtitle, fill=(90, 118, 150), anchor="mm")
    return img


def make_header(icon: Image.Image) -> Image.Image:
    img = vertical_gradient(HEADER)
    add_glow(img, (28, 28), 26, GLOW, 110)
    paste_icon(img, icon, (8, 8, 49, 49))
    draw = ImageDraw.Draw(img)
    draw.text((58, 28), "Aether", font=font(18, bold=True), fill=TEXT, anchor="lm")
    return img


def nsis_escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', r"$\"")


def write_urls(data: dict[str, str]) -> None:
    donate = (data.get("donateUrl") or "").strip()
    repo = (data.get("repoUrl") or "").strip()
    itch = (data.get("itchUrl") or "").strip()
    kofi = (data.get("koFiUrl") or "").strip()
    finish = donate or kofi or itch or repo
    (OUT_DIR / "urls.nsh").write_text(
        "\n".join(
            [
                f'!define AETHER_DONATE "{nsis_escape(finish)}"',
                f'!define AETHER_WEBSITE "{nsis_escape(repo)}"',
                f'!define AETHER_ITCH "{nsis_escape(itch)}"',
                f'!define AETHER_KOFI "{nsis_escape(kofi)}"',
                "",
            ]
        ),
        encoding="utf-8",
    )


def cls(url: str, extra: str = "") -> str:
    bits = ["hidden"] if not url else []
    if extra:
        bits.append(extra)
    return " ".join(bits).strip()


def write_support_html(data: dict[str, str]) -> None:
    template = (OUT_DIR / "support.template.html").read_text(encoding="utf-8")
    donate = (data.get("donateUrl") or "").strip()
    repo = (data.get("repoUrl") or "").strip()
    itch = (data.get("itchUrl") or "").strip()
    kofi = (data.get("koFiUrl") or "").strip()
    html = (
        template.replace("__REPO_URL__", repo or "#")
        .replace("__ITCH_URL__", itch or "#")
        .replace("__KOFI_URL__", kofi or "#")
        .replace("__DONATE_URL__", donate or "#")
        .replace("__GITHUB_CLASS__", cls(repo, "secondary"))
        .replace("__ITCH_CLASS__", cls(itch))
        .replace("__KOFI_CLASS__", cls(kofi, "secondary"))
        .replace("__DONATE_CLASS__", cls(donate, "secondary"))
    )
    (OUT_DIR / "support.html").write_text(html, encoding="utf-8")
    version = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
    (OUT_DIR / ".itch.toml").write_text(
        "\n".join(
            [
                "[[actions]]",
                'name = "install"',
                f'path = "Aether_{version}_x64-setup.exe"',
                'platform = "windows"',
                "",
            ]
        ),
        encoding="utf-8",
    )


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    links = json.loads((ROOT / "src" / "lib" / "distribution.json").read_text(encoding="utf-8"))
    write_urls(links)
    write_support_html(links)
    icon = load_icon()
    save_bmp24(make_sidebar(icon), OUT_DIR / "sidebar.bmp")
    header = make_header(icon)
    save_bmp24(header, OUT_DIR / "header.bmp")
    save_bmp24(header, OUT_DIR / "uninstaller-header.bmp")
    print(f"Wrote installer assets to {OUT_DIR}")


if __name__ == "__main__":
    main()
