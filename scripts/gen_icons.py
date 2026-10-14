#!/usr/bin/env python3
"""生成 Oxidict 的应用图标与托盘图标（可复现，无第三方依赖）。

产物：
  assets/icon.png            1024x1024 应用图标（圆角方形 + 双向箭头）
  assets/icon.iconset/*      iconutil 所需的各尺寸（由本脚本生成）
  assets/icon-tray-44.rgba   44x44  菜单栏模板图标（纯黑 + alpha，供 macOS 模板渲染）
  assets/icon-tray-88.rgba   88x88  @2x

为什么用脚本而不是引入图片资源：菜单栏图标要在 16pt 下依然清晰，几何字形
（1x/2x 精确对齐 + 4x 超采样抗锯齿）比位图缩放的观感稳定得多，而且可复现、
可审阅。托盘图标直接产出**裸 RGBA**，这样 Rust 侧 include_bytes! 即可喂给
`tray_icon::Icon::from_rgba`，无需在 UI 层引入图像解码依赖。

用法：
    python3 scripts/gen_icons.py            # 生成 PNG 与 RGBA
    iconutil -c icns assets/icon.iconset -o assets/icon.icns
"""

from __future__ import annotations

import os
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "assets")

# 品牌色（与原托盘占位色一致：#2EC27E 系）。
BRAND_TOP = (0x4A, 0xDE, 0xA0)
BRAND_BOTTOM = (0x18, 0xA9, 0x6B)
SS = 4  # 超采样倍率


def _rounded_rect_coverage(x: float, y: float, w: float, h: float, r: float) -> float:
    """点 (x, y) 落在圆角矩形内的覆盖率（0..1，单点采样）。"""
    if x < 0.0 or y < 0.0 or x > w or y > h:
        return 0.0
    cx = min(max(x, r), w - r)
    cy = min(max(y, r), h - r)
    dx = x - cx
    dy = y - cy
    return 1.0 if dx * dx + dy * dy <= r * r else 0.0


def _in_triangle(px: float, py: float, ax: float, ay: float, bx: float, by: float, cx: float, cy: float) -> bool:
    def sign(x1, y1, x2, y2, x3, y3):
        return (x1 - x3) * (y2 - y3) - (x2 - x3) * (y1 - y3)

    d1 = sign(px, py, ax, ay, bx, by)
    d2 = sign(px, py, bx, by, cx, cy)
    d3 = sign(px, py, cx, cy, ax, ay)
    has_neg = d1 < 0 or d2 < 0 or d3 < 0
    has_pos = d1 > 0 or d2 > 0 or d3 > 0
    return not (has_neg and has_pos)


def _glyph_cov(nx: float, ny: float) -> float:
    """归一化坐标 (0..1) 处的字形覆盖率：两个反向箭头（translate/swap 语义）。"""
    bar = 0.085          # 杆厚
    tip = 0.105          # 箭头长度
    x0, x1 = 0.235, 0.735
    y_top, y_bot = 0.365, 0.635

    # 上杆（右向）
    if x0 <= nx <= x1 and abs(ny - y_top) <= bar / 2:
        return 1.0
    # 上箭头
    ax, ay = x1 + tip, y_top
    if _in_triangle(nx, ny, x1, y_top - 0.115, x1, y_top + 0.115, ax, ay):
        return 1.0
    # 下杆（左向）
    if x0 <= nx <= x1 and abs(ny - y_bot) <= bar / 2:
        return 1.0
    # 下箭头
    ax2, ay2 = x0 - tip, y_bot
    if _in_triangle(nx, ny, x0, y_bot - 0.115, x0, y_bot + 0.115, ax2, ay2):
        return 1.0
    return 0.0


def render(size: int, *, badge: bool) -> bytes:
    """渲染 size×size 的 RGBA。

    badge=True  → 彩色圆角方形徽章 + 白色字形（应用图标）
    badge=False → 透明背景 + 纯黑字形（macOS 菜单栏模板图标，颜色由系统决定）
    """
    out = bytearray()
    inv = 1.0 / (size * SS)
    for py in range(size):
        for px in range(size):
            badge_cov = 0.0
            glyph_cov = 0.0
            for sy in range(SS):
                for sx in range(SS):
                    nx = (px * SS + sx + 0.5) * inv
                    ny = (py * SS + sy + 0.5) * inv
                    if badge:
                        # 徽章占 12% 边距，圆角半径 22%（接近 macOS squircle）。
                        pad = 0.12
                        badge_cov += _rounded_rect_coverage(
                            nx - pad, ny - pad, 1.0 - 2 * pad, 1.0 - 2 * pad, 0.225
                        )
                    glyph_cov += _glyph_cov(nx, ny)
            n = float(SS * SS)
            badge_cov /= n
            glyph_cov /= n

            if badge:
                # 竖向渐变底色，字形以白色覆盖在上面。
                t = py / max(size - 1, 1)
                r = BRAND_TOP[0] + (BRAND_BOTTOM[0] - BRAND_TOP[0]) * t
                g = BRAND_TOP[1] + (BRAND_BOTTOM[1] - BRAND_TOP[1]) * t
                b = BRAND_TOP[2] + (BRAND_BOTTOM[2] - BRAND_TOP[2]) * t
                a = badge_cov
                if glyph_cov > 0.0:
                    r = r + (255.0 - r) * glyph_cov
                    g = g + (255.0 - g) * glyph_cov
                    b = b + (255.0 - b) * glyph_cov
                out += bytes((int(round(r)), int(round(g)), int(round(b)), int(round(a * 255))))
            else:
                out += bytes((0, 0, 0, int(round(glyph_cov * 255))))
    return bytes(out)


def write_png(path: str, size: int, rgba: bytes, *, badge: bool) -> None:
    """写 8-bit RGBA PNG（行前置 filter 0）。"""
    raw = bytearray()
    stride = size * 4
    for y in range(size):
        raw.append(0)
        raw += rgba[y * stride : (y + 1) * stride]

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    color_type = 6  # RGBA
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, color_type, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def main() -> None:
    os.makedirs(ASSETS, exist_ok=True)

    # 应用图标：1024 主图 + iconset 各尺寸（iconutil 需要精确命名）。
    iconset = os.path.join(ASSETS, "icon.iconset")
    os.makedirs(iconset, exist_ok=True)
    for spec, px in [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ]:
        rgba = render(px, badge=True)
        write_png(os.path.join(iconset, spec), px, rgba, badge=True)
        if px == 1024:
            write_png(os.path.join(ASSETS, "icon.png"), px, rgba, badge=True)
        print(f"icon {px:>4}px -> {spec}")

    # 托盘模板图标：裸 RGBA，Rust 侧 include_bytes! 直接用。
    for px in (44, 88):
        rgba = render(px, badge=False)
        path = os.path.join(ASSETS, f"icon-tray-{px}.rgba")
        with open(path, "wb") as f:
            f.write(rgba)
        print(f"tray {px:>4}px -> {os.path.basename(path)} ({len(rgba)} bytes)")

    print("\n下一步：iconutil -c icns assets/icon.iconset -o assets/icon.icns")


if __name__ == "__main__":
    main()
