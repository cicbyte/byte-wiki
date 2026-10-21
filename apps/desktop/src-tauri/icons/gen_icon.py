"""
生成 byte-wiki 任务栏图标（靛紫渐变底 + 白色翻开书本）

输出：
  icon.png  — 512x512 高清 PNG
  icon.ico  — 多尺寸 ICO（16/24/32/48/64/128/256），供 Tauri 打包

设计：byte-wiki 是仓库级知识引擎（Wiki）——标识为「翻开的书」。
靛紫渐变底（#6366F1 → #8B5CF6）与 byte-stash（纯蓝 #3B82F6 + 层叠卡片）
一眼区分（层叠卡片是 stash 语义，旧图标为 fork 继承）。白色书页摊开、
中缝留隙；大尺寸页面带文字行示意，≤24px 简化为纯书页保证可辨。

与软件内 Logo.vue（透明动态着色书本）语义一致。
"""
from PIL import Image, ImageDraw

TOP = (99, 102, 241, 255)      # #6366F1 靛
BOTTOM = (139, 92, 246, 255)   # #8B5CF6 紫
WHITE = (255, 255, 255, 255)
GAP = (109, 106, 240, 255)     # 中缝色（近底色，压一档区分两页）


def vertical_gradient(size: int, top, bottom) -> Image.Image:
    img = Image.new("RGBA", (size, size))
    px = img.load()
    for y in range(size):
        t = y / max(size - 1, 1)
        c = tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(4))
        for x in range(size):
            px[x, y] = c
    return img


def rounded_mask(size: int, radius: int) -> Image.Image:
    m = Image.new("L", (size, size), 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, size, size], radius=radius, fill=255)
    return m


def make_logo(size: int) -> Image.Image:
    """指定尺寸图标：渐变圆角底 + 白色翻开书本（64 坐标系设计，超采样抗锯齿）"""
    SS = 4
    S = size * SS
    k = S / 64

    # 1. 渐变圆角底（Windows 惯例圆角约 18-22%）
    corner_r = (12 if size <= 24 else 14) * k
    grad = vertical_gradient(S, TOP, BOTTOM)
    canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    canvas.paste(grad, (0, 0), rounded_mask(S, int(corner_r)))

    d = ImageDraw.Draw(canvas)

    # 2. 翻开的书：两页摊开（梯形页 + 底部 V 形书脊），中缝留隙
    #    64 坐标系：书宽 40（12~52），页顶 y=18，页底 y=46，中缝在 x=32
    page_top_out, page_top_in = 14 * k, 19 * k   # 外沿高、内沿略低（书页自然弯曲）
    page_bot = 46 * k
    spine_dip = 49 * k                            # 中缝底部下沉成 V

    # 左页（外沿 (12,18) → 内沿 (31,19) → 内底 (31,46+沉降) → 外底 (12,46)）
    left = [
        (12 * k, page_top_out + 2 * k),
        (31 * k, page_top_in),
        (31 * k, page_bot),
        (12 * k, page_bot - 1 * k),
    ]
    # 微弧页面：外底略抬、内底沿 V 下沉
    left[2] = (31 * k, spine_dip - 2 * k)
    right = [(S - x, y) for (x, y) in left]      # 右页镜像

    d.polygon(left, fill=WHITE)
    d.polygon(right, fill=WHITE)

    # 3. 文字行（仅大尺寸）：靛紫色细线在白页上，左右各 2 行
    if size > 24:
        ink = (99, 102, 241, 200)
        for i, (x0, x1, y) in enumerate([
            (17 * k, 27 * k, 24 * k), (17 * k, 24 * k, 29 * k),   # 左页两行
            (37 * k, 47 * k, 24 * k), (40 * k, 47 * k, 29 * k),   # 右页两行（错位）
        ]):
            d.rounded_rectangle(
                [x0, y, x1, y + 1.8 * k],
                radius=0.9 * k, fill=ink,
            )

    return canvas.resize((size, size), Image.LANCZOS)


def main():
    import os
    here = os.path.dirname(os.path.abspath(__file__))
    # 高清 PNG
    make_logo(512).save(os.path.join(here, "icon.png"))
    # 多尺寸 ICO
    sizes = [16, 24, 32, 48, 64, 128, 256]
    imgs = [make_logo(s) for s in sizes]
    imgs[-1].save(
        os.path.join(here, "icon.ico"),
        format="ICO",
        append_images=imgs[:-1],
    )
    print(f"已生成 icon.png(512) 与 icon.ico({sizes})")


if __name__ == "__main__":
    main()
