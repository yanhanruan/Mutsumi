"""
睡眠 Z 粒子特效

当宠物心情为 SLEEPY（能量低于 20）时，在头顶生成浮动 Z 字符。
每隔 1.5 秒生成一个粒子，粒子在 2 秒内上升 30px 并淡出。

淡出通过将文字颜色从白色渐变到色键色（#FF00FF）实现，
完全兼容 -transparentcolor 透明方案，无需额外 -alpha 属性。
"""
from __future__ import annotations
import random
import time
import tkinter as tk
from typing import List, Optional, Tuple

from animator import CHROMA_HEX, WINDOW_SIZE


class ZParticles:
    """
    单一透明 Toplevel 容器内的浮动 Z 粒子系统。
    调用 start() 激活，stop() 立即销毁所有粒子。
    容器窗口每帧跟随宠物主窗口移动。
    """

    _LIFE_MS  = 2000   # 每个粒子的存活时间（ms）
    _SPAWN_MS = 1500   # 粒子生成间隔（ms）
    _RISE_PX  =   30   # 粒子在生命周期内上升的像素数
    _STEP_MS  =   50   # 动画更新间隔（ms，约 20fps）
    _CANVAS_W =   80   # 容器宽度（px）
    _CANVAS_H =   50   # 容器高度（px）
    _FONT     = ('Segoe UI', 9, 'bold')
    _Z_TEXTS  = ('z', 'Z', 'Zzz')

    def __init__(self, root: tk.Tk) -> None:
        self._root      = root
        self._win:      Optional[tk.Toplevel]                     = None
        self._canvas:   Optional[tk.Canvas]                       = None
        # 每条记录：(canvas_item_id, start_monotonic, spawn_x, spawn_y_canvas)
        self._particles: List[Tuple[int, float, int, int]]        = []
        self._spawn_job: Optional[str]                            = None
        self._anim_job:  Optional[str]                            = None
        self._z_idx:     int                                      = 0

    @property
    def active(self) -> bool:
        return self._win is not None

    # ── 公开接口 ──────────────────────────────────────────────────────────

    def start(self) -> None:
        """激活粒子系统（重复调用无副作用）"""
        if self.active:
            return
        self._win = tk.Toplevel(self._root)
        self._win.overrideredirect(True)
        self._win.attributes('-topmost', True)
        self._win.attributes('-transparentcolor', CHROMA_HEX)
        self._win.config(bg=CHROMA_HEX)
        self._canvas = tk.Canvas(
            self._win,
            width=self._CANVAS_W, height=self._CANVAS_H,
            bg=CHROMA_HEX, highlightthickness=0,
        )
        self._canvas.pack()
        self._reposition()
        self._spawn_z()     # 立即生成第一个粒子
        self._anim_step()   # 启动动画循环

    def stop(self) -> None:
        """立即停止并销毁所有粒子（容器窗口一并销毁）"""
        if self._spawn_job:
            self._root.after_cancel(self._spawn_job)
            self._spawn_job = None
        if self._anim_job:
            self._root.after_cancel(self._anim_job)
            self._anim_job = None
        if self._win:
            self._win.destroy()
            self._win    = None
            self._canvas = None
        self._particles.clear()

    # ── 内部逻辑 ──────────────────────────────────────────────────────────

    def _reposition(self) -> None:
        """将容器窗口的底部对齐到宠物头顶锚点，水平居中"""
        if not self._win:
            return
        wx = self._root.winfo_x()
        wy = self._root.winfo_y()
        cx = wx + WINDOW_SIZE // 2 - self._CANVAS_W // 2
        cy = wy + 28 - self._CANVAS_H   # 28 = bubble_anchor Y 偏移（头顶）
        self._win.geometry(f'+{cx}+{cy}')

    def _spawn_z(self) -> None:
        if not self._win or not self._canvas:
            return
        text   = self._Z_TEXTS[self._z_idx % len(self._Z_TEXTS)]
        self._z_idx += 1
        x      = random.randint(12, self._CANVAS_W - 12)
        y_base = self._CANVAS_H - 6     # 在容器底部附近生成
        item   = self._canvas.create_text(
            x, y_base,
            text=text, fill='#FFFFFF', font=self._FONT,
        )
        self._particles.append((item, time.monotonic(), x, y_base))
        self._spawn_job = self._root.after(self._SPAWN_MS, self._spawn_z)

    def _anim_step(self) -> None:
        if not self._win or not self._canvas:
            return
        self._reposition()
        now   = time.monotonic()
        alive: List[Tuple[int, float, int, int]] = []
        for item, start_t, x, y_base in self._particles:
            t = (now - start_t) / (self._LIFE_MS / 1000.0)
            if t >= 1.0:
                self._canvas.delete(item)
                continue
            y     = y_base - int(t * self._RISE_PX)
            color = self._fade_color(t)
            self._canvas.coords(item, x, y)
            self._canvas.itemconfig(item, fill=color)
            alive.append((item, start_t, x, y_base))
        self._particles = alive
        self._anim_job = self._root.after(self._STEP_MS, self._anim_step)

    @staticmethod
    def _fade_color(t: float) -> str:
        """白色 #FFFFFF → 色键色 #FF00FF；只需将绿通道从 255 衰减到 0"""
        g = int(255 * (1.0 - t))
        return f'#FF{g:02X}FF'
