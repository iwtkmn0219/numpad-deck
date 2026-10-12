"""앱 아이콘을 만든다 (app/src-tauri/icons/). 차콜 바탕 + 3×3 키캡 + 오른쪽 위(넘패드 9) 한 칸만 파랑.

    python -m pip install pillow      # 필요하면 가상 환경에
    python app/scripts/make-icons.py

48px 이상은 icon.svg와 같은 도형(256 기준)을 그 크기로 그린다. 작은 크기(16~44)는 큰 그림을
줄이지 않고 픽셀 격자에 맞춰 따로 그린다 (줄이면 칸 사이가 번진다). 값은 디자인 요청서의 표.
"""

from pathlib import Path

from PIL import Image, ImageDraw

ICONS = Path(__file__).resolve().parents[1] / "src-tauri" / "icons"

BG = "#1F1E1C"
FACE, EDGE = "#5C5751", "#3A3733"  # 키캡 면 / 아랫면
LIT_FACE, LIT_EDGE = "#3B82F6", "#1D4ED8"  # 켜진 키(오른쪽 위)
FLAT = "#6E6963"  # 아랫면 없는 작은 크기의 키캡 (한 단 밝게)
SS = 8  # 둥근 모서리용 확대 배율

# 크기: (여백, 칸, 간격, 바탕 둥글기, 칸 모서리, 아랫면)
#   칸 모서리: "square" 각진 사각형, "cut" 네 귀퉁이 1px 비움, 숫자 = 둥글기
TABLE = {
    16: (1, 4, 1, 3, "square", 0),
    20: (2, 4, 2, 4, "square", 0),
    24: (2, 6, 1, 5, "cut", 0),
    30: (3, 6, 3, 7, "cut", 0),  # Store 30 = 32 규칙
    32: (4, 6, 3, 7, "cut", 0),
    40: (5, 8, 3, 8, 2, 2),  # 48 규칙
    44: (4, 10, 3, 9, 2, 2),  # Store 44 = 48 규칙
    48: (6, 10, 3, 10, 2, 2),
    64: (8, 14, 3, 14, 3, 2),
    128: (20, 26, 5, 28, 6, 4),
    256: (44, 48, 12, 56, 10, 6),
}


def lit(row: int, col: int) -> bool:
    return row == 0 and col == 2


def draw_table(size: int) -> Image.Image:
    margin, cell, gap, bg_r, corner, edge = TABLE[size]
    assert margin * 2 + cell * 3 + gap * 2 == size, size
    big = Image.new("RGBA", (size * SS, size * SS), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)
    d.rounded_rectangle((0, 0, size * SS - 1, size * SS - 1), radius=bg_r * SS, fill=BG)
    rounded = isinstance(corner, int)
    if rounded:
        for row in range(3):
            for col in range(3):
                x = (margin + col * (cell + gap)) * SS
                y = (margin + row * (cell + gap)) * SS
                face, under = (LIT_FACE, LIT_EDGE) if lit(row, col) else (FACE, EDGE)
                r = corner * SS
                d.rounded_rectangle((x, y, x + cell * SS - 1, y + cell * SS - 1), radius=r, fill=under)
                d.rounded_rectangle((x, y, x + cell * SS - 1, y + (cell - edge) * SS - 1), radius=r, fill=face)
    img = big.resize((size, size), Image.LANCZOS)
    if not rounded:
        # 작은 크기: 칸은 안티에일리어싱 없이 픽셀로
        px = img.load()
        for row in range(3):
            for col in range(3):
                x0 = margin + col * (cell + gap)
                y0 = margin + row * (cell + gap)
                color = Image.new("RGBA", (1, 1), LIT_FACE if lit(row, col) else FLAT).getpixel((0, 0))
                for y in range(y0, y0 + cell):
                    for x in range(x0, x0 + cell):
                        is_corner = x in (x0, x0 + cell - 1) and y in (y0, y0 + cell - 1)
                        if corner == "cut" and is_corner:
                            continue
                        px[x, y] = color
    return img


def draw_svg(size: int) -> Image.Image:
    """icon.svg(256 기준)를 그 크기로 그린다."""
    s = size * SS / 256
    big = Image.new("RGBA", (size * SS, size * SS), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)
    d.rounded_rectangle((0, 0, size * SS - 1, size * SS - 1), radius=56 * s, fill=BG)
    for row in range(3):
        for col in range(3):
            x, y = (44 + col * 60) * s, (44 + row * 60) * s
            face, under = (LIT_FACE, LIT_EDGE) if lit(row, col) else (FACE, EDGE)
            d.rounded_rectangle((x, y, x + 48 * s - 1, y + 48 * s - 1), radius=10 * s, fill=under)
            d.rounded_rectangle((x, y, x + 48 * s - 1, y + 42 * s - 1), radius=10 * s, fill=face)
    return big.resize((size, size), Image.LANCZOS)


def icon(size: int) -> Image.Image:
    return draw_table(size) if size in TABLE else draw_svg(size)


def main() -> None:
    pngs = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
        "Square30x30Logo.png": 30,
        "Square44x44Logo.png": 44,
        "Square71x71Logo.png": 71,
        "Square89x89Logo.png": 89,
        "Square107x107Logo.png": 107,
        "Square142x142Logo.png": 142,
        "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284,
        "Square310x310Logo.png": 310,
        "StoreLogo.png": 50,
    }
    for name, size in pngs.items():
        icon(size).save(ICONS / name)

    ico_sizes = [16, 20, 24, 32, 40, 48, 64, 256]
    images = [icon(s) for s in ico_sizes]
    images[-1].save(
        ICONS / "icon.ico",
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=images[:-1],
    )

    # macOS(범위 밖)도 Tauri 로고가 남지 않게
    draw_svg(1024).save(ICONS / "icon.icns")
    print("ok", ICONS)


if __name__ == "__main__":
    main()
