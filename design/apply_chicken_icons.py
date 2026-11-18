"""把两只鸭抠出来存证，并把两只新鸡灌进仓库的图标源图位置。

- 抠图：从四角泛洪填充，把"瓷贴外的背景"变透明，主体连通域原样保留。
- 不修改任何已有文件：原始两张鸭另存到 design/icons-original/ 作为对照。
"""

from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
PUBLIC = ROOT / "public"
ICONS = ROOT / "src-tauri" / "icons"
RAW = ROOT / "design" / "icons-raw"
ORIGINAL = ROOT / "design" / "icons-original"
EXTRACTED = ROOT / "design" / "extracted"

WATERCOLOR_CHICKEN = next(RAW.glob("*水彩*2026-11-12T07-28-27.png"))
FACETED_CHICKEN = next(RAW.glob("*low_poly*2026-11-12T07-28-51.png"))

SOURCES = {
    "duck-standard": PUBLIC / "app-logo.png",
    "duck-faceted": PUBLIC / "app-icon-faceted-duck.png",
    "chicken-standard": WATERCOLOR_CHICKEN,
    "chicken-faceted": FACETED_CHICKEN,
}


def extract_subject(image: Image.Image, tolerance: int = 26) -> Image.Image:
    """四角泛洪填充成哨兵色，再把哨兵色置为透明，得到主体 PNG。"""
    rgba = image.convert("RGBA")
    flat = rgba.convert("RGB")
    sentinel = (255, 0, 255)
    width, height = flat.size
    for seed in ((0, 0), (width - 1, 0), (0, height - 1), (width - 1, height - 1)):
        ImageDraw.floodfill(flat, seed, sentinel, thresh=tolerance)
    out = rgba.copy()
    pixels = out.load()
    flat_pixels = flat.load()
    for y in range(height):
        for x in range(width):
            if flat_pixels[x, y] == sentinel:
                pixels[x, y] = (0, 0, 0, 0)
    return out


def save_webp(image: Image.Image, path: pathlib.Path, size: int) -> None:
    image.convert("RGBA").resize((size, size), Image.LANCZOS).save(
        path, "WEBP", quality=92, method=6
    )


def main() -> int:
    ORIGINAL.mkdir(parents=True, exist_ok=True)
    EXTRACTED.mkdir(parents=True, exist_ok=True)

    for name, path in SOURCES.items():
        if not path.exists():
            raise SystemExit(f"缺少源图：{path}")

    # 1. 原始两只鸭留档 + 抠出主体
    for name in ("duck-standard", "duck-faceted"):
        source = SOURCES[name]
        shutil.copy2(source, ORIGINAL / f"{name}.png")
        extracted = extract_subject(Image.open(source))
        extracted.save(EXTRACTED / f"{name}.png")
        print(f"抠图完成：{name}")

    # 2. 两只鸡同样抠一份（供你单独取用）
    for name in ("chicken-standard", "chicken-faceted"):
        extracted = extract_subject(Image.open(SOURCES[name]))
        extracted.save(EXTRACTED / f"{name}.png")
        print(f"抠图完成：{name}")

    # 3. 灌进仓库的图标源图位置（覆盖，原图已留档）
    Image.open(WATERCOLOR_CHICKEN).convert("RGBA").resize((1024, 1024), Image.LANCZOS).save(
        ICONS / "icon-source.png"
    )
    Image.open(FACETED_CHICKEN).convert("RGBA").resize((1024, 1024), Image.LANCZOS).save(
        ICONS / "app-icons" / "faceted-duck-source.png"
    )
    print("源图已替换：icon-source.png / faceted-duck-source.png")

    # 4. 「默认」风格的网页侧预览（侧栏与设置页直接引用 public 下的 webp）
    watercolor = Image.open(WATERCOLOR_CHICKEN).convert("RGBA")
    watercolor.resize((512, 512), Image.LANCZOS).save(PUBLIC / "app-logo.png")
    save_webp(watercolor, PUBLIC / "app-logo.webp", 256)
    print("public/app-logo.png + .webp 已更新")

    # 5. 跑仓库自带的两支派生脚本，保证尺寸档 / .ico / 满画布判据与原来一致
    python = sys.executable
    for script in ("generate-standard-icon.py", "generate-app-icons.py"):
        print(f"--- 运行 {script} ---")
        result = subprocess.run(
            [python, str(ROOT / "scripts" / script)], cwd=ROOT, text=True, capture_output=True
        )
        print(result.stdout.strip())
        if result.returncode != 0:
            print(result.stderr.strip())
            return result.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
