"""
动画帧生成器
从基础角色图像（RGBA）生成待机、点击、拖拽三种动画的所有帧

Animation 类
  封装帧列表（QPixmap 或 PIL Image）、fps 和循环标志。
  作为整个动画系统中"帧数据 + 播放规则"的统一载体。
"""
import math
from typing import List
from PIL import Image

# ── 尺寸常量 ──
WINDOW_SIZE = 200   # 宠物窗口像素大小
CHAR_SIZE   = 150   # 角色图像像素大小（占位符模式默认缩放目标）


# ── Animation 类 ─────────────────────────────────────────────────────────────

class Animation:
    """
    一个命名动画的完整描述：帧列表（QPixmap 或 PIL Image）、播放速率和循环标志。

    frames  — list of QPixmap (for directory-loaded animations) or PIL Image (for generated ones).
    fps     — 播放帧率（帧/秒），interval_ms 由此派生。
    loop    — True = 循环播放；False = 播完一轮后切回 'idle'。
    """

    def __init__(
        self,
        frames: list,
        fps:    float = 12.0,
        loop:   bool  = True,
    ) -> None:
        self.frames = frames
        self.fps    = fps
        self.loop   = loop

    @property
    def interval_ms(self) -> int:
        """每帧之间的延迟毫秒数（根据 fps 派生，最小 1 ms）"""
        return max(1, int(1000.0 / self.fps))


# ── 内部画布工具 ─────────────────────────────────────────────────────────────

def _make_canvas():
    return Image.new('RGBA', (WINDOW_SIZE, WINDOW_SIZE), (0, 0, 0, 0))

def _paste(canvas: Image.Image, char: Image.Image, ox: int = 0, oy: int = 0) -> None:
    """
    将角色图像居中粘贴到画布，支持偏移。
    RGBA 图像使用 alpha 通道做透明合成。
    """
    x = (WINDOW_SIZE - char.width)  // 2 + ox
    y = (WINDOW_SIZE - char.height) // 2 + oy

    if char.mode == 'RGBA':
        canvas.paste(char.convert('RGB'), (x, y), char.split()[3])
    else:
        canvas.paste(char, (x, y))


# ── Pillow 占位符帧生成器 ─────────────────────────────────────────────────────
# 用于非 idle 动画（click/drag/fly/dizzy/squash/rebound）的程序化帧生成。
# 每个函数返回 PIL Image 列表，由 pet.py 的 _pil_anim() 在启动时转换为 QPixmap。

def generate_idle_frames(char_img: Image.Image, count: int = 8) -> List[Image.Image]:
    """
    生成待机浮动动画帧
    使用正弦波实现上下 ±4 像素的平滑漂浮效果
    """
    char = char_img.resize((CHAR_SIZE, CHAR_SIZE), Image.LANCZOS)
    frames = []
    for i in range(count):
        offset_y = int(math.sin(2 * math.pi * i / count) * 4)
        canvas = _make_canvas()
        _paste(canvas, char, oy=offset_y)
        frames.append(canvas)
    return frames


def generate_click_frames(char_img: Image.Image, count: int = 6) -> List[Image.Image]:
    """
    生成点击抖动动画帧
    预定义的左右抖动偏移序列，模拟受击震动感
    """
    char = char_img.resize((CHAR_SIZE, CHAR_SIZE), Image.LANCZOS)
    shake = [0, -8, 8, -5, 5, 0]
    frames = []
    for i in range(count):
        canvas = _make_canvas()
        _paste(canvas, char, ox=shake[i % len(shake)])
        frames.append(canvas)
    return frames


def generate_drag_frames(char_img: Image.Image, count: int = 4) -> List[Image.Image]:
    """
    生成拖拽旋转动画帧
    轻微旋转角色模拟被拖动时的倾斜感
    """
    char   = char_img.resize((CHAR_SIZE, CHAR_SIZE), Image.LANCZOS).convert('RGBA')
    angles = [-10, -5, 5, 10]
    frames = []
    for i in range(count):
        rotated = char.rotate(angles[i % len(angles)], resample=Image.BICUBIC, expand=False)
        canvas  = _make_canvas()
        _paste(canvas, rotated)
        frames.append(canvas)
    return frames


def generate_fly_frames(char_img: Image.Image, count: int = 16) -> List[Image.Image]:
    """
    生成飞行翻转帧：均匀覆盖 360° 旋转。
    帧 i 对应顺时针旋转 i×(360/count)°，_fly_step() 根据速度方向向量
    直接索引此列表，实现"朝速度方向翻滚"的视觉效果。
    """
    char = char_img.resize((CHAR_SIZE, CHAR_SIZE), Image.LANCZOS).convert('RGBA')
    frames = []
    for i in range(count):
        angle   = -360.0 * i / count
        rotated = char.rotate(angle, resample=Image.BICUBIC, expand=False)
        canvas  = _make_canvas()
        _paste(canvas, rotated)
        frames.append(canvas)
    return frames


def generate_squash_frames(char_img: Image.Image) -> tuple:
    """
    生成挤压弹回帧对（ease-out cubic）：
      squash（5帧）：1.0×1.0 → 1.15w × 0.90h
      rebound（4帧）：1.15w × 0.90h → 1.0×1.0
    返回 (squash_list, rebound_list)，每个列表均为 PIL.Image。
    """
    def ease_out(t: float) -> float:
        return 1.0 - (1.0 - t) ** 3

    char = char_img.resize((CHAR_SIZE, CHAR_SIZE), Image.LANCZOS).convert('RGBA')

    def make_frame(sx: float, sy: float) -> Image.Image:
        w      = max(1, int(CHAR_SIZE * sx))
        h      = max(1, int(CHAR_SIZE * sy))
        scaled = char.resize((w, h), Image.LANCZOS)
        canvas = _make_canvas()
        _paste(canvas, scaled)
        return canvas

    N_SQ, N_RB = 5, 4
    squash  = [make_frame(1.0 + 0.15 * ease_out(i / (N_SQ - 1)),
                          1.0 - 0.10 * ease_out(i / (N_SQ - 1))) for i in range(N_SQ)]
    rebound = [make_frame(1.15 - 0.15 * ease_out(i / (N_RB - 1)),
                          0.90 + 0.10 * ease_out(i / (N_RB - 1))) for i in range(N_RB)]
    return squash, rebound


def generate_dizzy_frames(char_img: Image.Image, count: int = 8) -> List[Image.Image]:
    """
    生成眩晕摇晃动画帧：大幅度左右交替摆动，最终回正。
    """
    char   = char_img.resize((CHAR_SIZE, CHAR_SIZE), Image.LANCZOS).convert('RGBA')
    angles = [-22, 22, -16, 16, -10, 10, -4, 0]
    frames = []
    for i in range(count):
        rotated = char.rotate(angles[i % len(angles)], resample=Image.BICUBIC, expand=False)
        canvas  = _make_canvas()
        _paste(canvas, rotated)
        frames.append(canvas)
    return frames

