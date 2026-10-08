"""
生成 byte-wiki 任务栏图标（蓝色方块底 + 白色层叠卡片）

输出：
  icon.png  — 512x512 高清 PNG
  icon.ico  — 多尺寸 ICO（16/24/32/48/64/128/256），供 Tauri 打包

设计：蓝色圆角方块底（#3B82F6）铺满画布，上面叠三张白色卡片错落旋转。
白色卡片在蓝底上用透明度分层（0.5/0.75/1.0），半透明白在蓝色上呈浅蓝，
层次清晰且不会像透明背景版那样"脏"。蓝底保证任何 Windows 任务栏背景下都醒目。

与软件内 Logo.vue（透明动态着色版）语义一致——都是"层叠卡片"，
只是任务栏静态位图需要底色保证可见性。
"""
from PIL import Image, ImageDraw

PRIMARY = (59, 130, 246, 255)   # #3B82F6 蓝色底
WHITE = (255, 255, 255, 255)    # 白色卡片


def _with_alpha(color_rgba, opacity):
    r, g, b, _ = color_rgba
    return (r, g, b, int(255 * opacity))


def make_logo(size: int) -> Image.Image:
    """
    生成指定尺寸的图标：蓝底圆角方块 + 白色层叠卡片。

    所有尺寸统一设计（不再分 simple/full）——蓝底保证了小尺寸也清晰，
    不需要简化版。卡片靠旋转角度错落 + 透明度分层。
    """
    SS = 4  # 超采样倍数
    S = size * SS
    canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(canvas)
    k = S / 64

    # 1. 蓝色圆角方块底，铺满画布（圆角随尺寸：大尺寸圆角大，小尺寸接近直角）
    # Windows 图标圆角惯例：约画布 18% 的半径。小尺寸（16px）适当收小避免变形。
    if size <= 24:
        corner_r = 12 * k   # 小尺寸稍小圆角
    else:
        corner_r = 14 * k
    d.rounded_rectangle([0, 0, S, S], radius=corner_r, fill=PRIMARY)

    # 2. 白色层叠卡片：三张错落旋转，居中
    # 卡片参数（64 坐标系）：宽 30、高 36，居中放置。旋转中心 (32,32)。
    card_w, card_h = 30 * k, 36 * k
    card_x = (S - card_w) / 2   # = 17k
    card_y = (S - card_h) / 2   # = 14k
    cx = cy = S / 2
    card_r = 3 * k              # 卡片圆角

    # 后卡片（旋转 -12°，opacity 0.5）—— 在蓝底上呈浅蓝
    back = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(back).rounded_rectangle(
        [card_x, card_y, card_x + card_w, card_y + card_h],
        radius=card_r, fill=_with_alpha(WHITE, 0.5),
    )
    back = back.rotate(-12, resample=Image.BICUBIC, expand=False, center=(cx, cy))

    # 中卡片（旋转 -3°，opacity 0.78）
    mid = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(mid).rounded_rectangle(
        [card_x, card_y, card_x + card_w, card_y + card_h],
        radius=card_r, fill=_with_alpha(WHITE, 0.78),
    )
    mid = mid.rotate(-3, resample=Image.BICUBIC, expand=False, center=(cx, cy))

    # 前卡片（不旋转，实色白 + 标题行挖空露出蓝底）
    front = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    fd = ImageDraw.Draw(front)
    fd.rounded_rectangle(
        [card_x, card_y, card_x + card_w, card_y + card_h],
        radius=card_r, fill=WHITE,
    )
    # 挖空两条标题行（透明 → 露出蓝底），相对卡片左上角的偏移
    # 卡片 x=17k，内边距 5k → 标题行 x=22k；卡片宽 30k，标题行宽 20k/14k
    # 卡片 y=14k，标题行 y=24k、30k（卡片上 1/3~中部）
    fd.rectangle([22 * k, 24 * k, (22 + 20) * k, (24 + 2.4) * k], fill=(0, 0, 0, 0))
    fd.rectangle([22 * k, 30 * k, (22 + 14) * k, (30 + 2.4) * k], fill=(0, 0, 0, 0))

    # 合成：蓝底 → 后卡片 → 中卡片 → 前卡片
    canvas.alpha_composite(back)
    canvas.alpha_composite(mid)
    canvas.alpha_composite(front)

    return canvas.resize((size, size), Image.LANCZOS)


def main():
    import os
    out_dir = os.path.dirname(os.path.abspath(__file__))

    # 高清 PNG（512）
    big = make_logo(512)
    big.save(os.path.join(out_dir, "icon.png"))
    print("✓ icon.png (512x512)")

    # ICO 多尺寸：传一张 256x256 高清图 + sizes 列表，Pillow 自动缩放嵌入各尺寸
    ico_sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    ico_src = make_logo(256)
    ico_src.save(
        os.path.join(out_dir, "icon.ico"),
        format="ICO",
        sizes=ico_sizes,
    )
    print(f"✓ icon.ico ({[s[0] for s in ico_sizes]})")


if __name__ == "__main__":
    main()
