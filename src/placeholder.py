"""
占位符角色生成器
当 assets/ 目录中没有提供角色图像时，自动生成一个可爱的动漫风格小女生
参考 Wakaba Mutsumi（结束睦）的形象设计
"""
from PIL import Image, ImageDraw


def create_placeholder_character() -> Image.Image:
    """
    生成 200×200 像素的 RGBA 角色图像（透明背景）
    无需外部字体或素材，纯用 Pillow 绘图 API 完成
    """
    size = 200
    img  = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    cx   = 100      # 水平中心像素

    # ── 皮肤色 ──
    skin = (255, 218, 185)

    # ─── 腿部 ───
    draw.rectangle([83, 155, 95,  185], fill=skin)   # 左腿
    draw.rectangle([105, 155, 117, 185], fill=skin)  # 右腿
    # 鞋子
    draw.ellipse([79, 179,  99, 192], fill=(48, 32, 18))   # 左鞋
    draw.ellipse([101, 179, 121, 192], fill=(48, 32, 18))  # 右鞋

    # ─── 裙子（粉紫） ───
    draw.polygon([80, 110, 72, 158, 128, 158, 120, 110], fill=(175, 95, 138))

    # ─── 上衣（白色制服） ───
    draw.polygon([82, 88, 78, 112, 122, 112, 118, 88], fill=(235, 235, 248))
    draw.polygon([92, 88, cx, 101, 108, 88], fill=(210, 210, 232))   # V 领
    # 领结
    draw.polygon([cx-5, 95, cx, 104, cx+5, 95, cx, 99], fill=(200, 70, 90))

    # ─── 手臂 ───
    draw.ellipse([67, 88,  82, 118], fill=skin)   # 左臂
    draw.ellipse([118, 88, 133, 118], fill=skin)  # 右臂

    # ─── 头发（后层，深棕） ───
    hair = (65, 40, 15)
    draw.ellipse([57, 22, 143, 92], fill=hair)          # 头发主体
    draw.rectangle([60, 60, 140, 100], fill=hair)        # 向下延伸

    # ─── 脖子 ───
    draw.rectangle([92, 82, 108, 92], fill=skin)

    # ─── 脸（椭圆） ───
    draw.ellipse([58, 27, 142, 110], fill=skin)

    # ─── 刘海（前层） ───
    bang_pts = [
        55, 42,   60, 63,   73, 55,
        87, 37,  100, 45,  113, 37,
        127, 55, 140, 63,  145, 42,
        145, 28,  55, 28
    ]
    draw.polygon(bang_pts, fill=hair)

    # ─── 发饰（小粉花） ───
    draw.ellipse([131, 26, 145, 40], fill=(255, 175, 205))   # 花瓣
    draw.ellipse([135, 30, 141, 36], fill=(255, 245, 195))   # 花蕊

    # ─── 眼睛（大眼紫虹膜） ───
    eye_y = 73          # 眼睛中心 Y
    for ex in [83, 117]:    # 左眼 / 右眼 X
        draw.ellipse([ex-10, eye_y-9, ex+10, eye_y+9],  fill=(255, 255, 255))  # 眼白
        draw.ellipse([ex-7,  eye_y-7, ex+7,  eye_y+7],  fill=(112, 72, 158))   # 虹膜
        draw.ellipse([ex-4,  eye_y-4, ex+4,  eye_y+4],  fill=(22,  14,  32))   # 瞳孔
        draw.ellipse([ex-5,  eye_y-8, ex-1,  eye_y-4],  fill=(255, 255, 255))  # 主高光
        draw.ellipse([ex+1,  eye_y-2, ex+4,  eye_y+1],  fill=(255, 255, 255))  # 副高光

    # ─── 鼻子（小点） ───
    draw.ellipse([cx-1, 89, cx+1, 92], fill=(210, 155, 120))

    # ─── 嘴巴（弧形微笑） ───
    draw.arc([cx-9, 91, cx+9, 103], start=0, end=180, fill=(190, 100, 100), width=2)

    # ─── 腮红（半透明椭圆，用 alpha 合成） ───
    blush_img  = Image.new('RGBA', (20, 9), (0, 0, 0, 0))
    blush_draw = ImageDraw.Draw(blush_img)
    blush_draw.ellipse([(0, 0), (19, 8)], fill=(255, 150, 150, 115))
    img.alpha_composite(blush_img, (63, 80))    # 左腮红
    img.alpha_composite(blush_img, (117, 80))   # 右腮红

    return img
