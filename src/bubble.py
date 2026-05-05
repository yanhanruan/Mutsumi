"""
语音气泡模块
显示角色头顶的对话气泡，并从 assets/lines.json 中按分类随机抽取台词。

动画行为
  显示：透明度 0 → 1（200ms，线性）+ 向上平移 8px（同步滑入）
  隐藏：透明度 1 → 0（150ms，线性）后销毁窗口
  替换：新气泡出现时立即销毁旧气泡（跳过淡出，避免两窗口并存）
"""
from __future__ import annotations
import json
import os
import random
import time
import tkinter as tk
from typing import Dict, List, Optional


# ── 兜底台词（lines.json 加载失败时使用） ──
_FALLBACK_LINES: Dict[str, List[str]] = {
    "idle":      ["Hmm~"],
    "clicked":   ["Yes?"],
    "dragged":   ["Woah!"],
    "morning":   ["Good morning!"],
    "afternoon": ["Good afternoon!"],
    "evening":   ["Good evening!"],
    "night":     ["Good night~"],
    "bored":     ["Notice me please~"],
}

_FADE_IN_MS  = 200    # 淡入总时长（ms）
_FADE_OUT_MS = 150    # 淡出总时长（ms）
_SLIDE_PX    =   8    # 淡入时向上滑入的像素数
_STEP_MS     =  16    # 动画帧间隔（约 60fps）


class SpeechBubble:
    """
    角色头顶的对话气泡（独立无边框 Toplevel 窗口）。
    显示时淡入 + 上滑；隐藏时淡出后销毁。
    """

    BG_COLOR     = '#FFFDE7'
    BORDER_COLOR = '#F9A825'
    TEXT_COLOR   = '#333333'
    FONT         = ('Segoe UI', 10)
    MAX_WIDTH    = 210

    def __init__(self, parent: tk.Tk, assets_dir: Optional[str] = None):
        self.parent      = parent
        self.window:     Optional[tk.Toplevel] = None
        self._hide_timer: Optional[str]        = None
        self._fade_job:   Optional[str]        = None
        self._fade_start: float                = 0.0
        self._final_bx:   int                  = 0
        self._final_by:   int                  = 0
        self._lines = self._load_lines(assets_dir)

    # ── 台词加载 ──────────────────────────────────────────────────────────

    def _load_lines(self, assets_dir: Optional[str]) -> Dict[str, List[str]]:
        if assets_dir:
            path = os.path.join(assets_dir, 'lines.json')
            try:
                with open(path, encoding='utf-8') as fh:
                    data = json.load(fh)
                print(f'[Bubble] Loaded lines.json ({sum(len(v) for v in data.values())} lines)')
                return data
            except Exception as exc:
                print(f'[Bubble] Could not load lines.json: {exc} — using fallback')
        return dict(_FALLBACK_LINES)

    # ── 台词选取 ──────────────────────────────────────────────────────────

    def pick(self, category: str) -> str:
        pool = self._lines.get(category) or self._lines.get('idle') or ['...']
        return random.choice(pool)

    # ── 公开显示接口 ──────────────────────────────────────────────────────

    def show_for(
        self,
        category: str,
        screen_x: int,
        screen_y: int,
        duration: int = 2000,
    ) -> None:
        self.show(self.pick(category), screen_x, screen_y, duration)

    def show(
        self,
        text: str,
        screen_x: int,
        screen_y: int,
        duration: int = 2000,
    ) -> None:
        """
        在屏幕坐标 (screen_x, screen_y) 上方显示气泡，淡入 + 上滑进入。
        若已有气泡，立即销毁（不等淡出）再创建新气泡。
        """
        self._destroy_now()   # 立即清除旧气泡，保证单气泡

        # ── 创建无边框顶层窗口 ──
        self.window = tk.Toplevel(self.parent)
        self.window.overrideredirect(True)
        self.window.attributes('-topmost', True)

        # ── 内容布局 ──
        outer = tk.Frame(self.window, bg=self.BORDER_COLOR, padx=2, pady=2)
        outer.pack()
        inner = tk.Frame(outer, bg=self.BG_COLOR, padx=10, pady=6)
        inner.pack()
        tk.Label(
            inner,
            text=text,
            font=self.FONT,
            fg=self.TEXT_COLOR,
            bg=self.BG_COLOR,
            wraplength=self.MAX_WIDTH - 24,
            justify='center',
        ).pack()

        # 刷新布局以获取真实尺寸
        self.window.update_idletasks()
        w = self.window.winfo_reqwidth()
        h = self.window.winfo_reqheight()

        # ── 计算最终位置 ──
        bx = screen_x - w // 2
        by = screen_y - h - 6
        if by < 0:
            by = screen_y + 6

        self._final_bx = bx
        self._final_by = by

        # 初始状态：全透明，位于最终位置下方 _SLIDE_PX 像素
        self.window.attributes('-alpha', 0.0)
        self.window.geometry(f'+{bx}+{by + _SLIDE_PX}')

        # 启动淡入动画
        self._fade_start = time.monotonic()
        self._fade_in_step()

        # 定时自动隐藏
        self._hide_timer = self.parent.after(duration, self.hide)

    # ── 淡入 ──────────────────────────────────────────────────────────────

    def _fade_in_step(self) -> None:
        if self.window is None:
            self._fade_job = None
            return
        t = min(1.0, (time.monotonic() - self._fade_start) / (_FADE_IN_MS / 1000.0))
        self.window.attributes('-alpha', t)
        # 同步上滑：从 final_by + _SLIDE_PX 移动到 final_by
        y = self._final_by + int(_SLIDE_PX * (1.0 - t))
        self.window.geometry(f'+{self._final_bx}+{y}')
        if t < 1.0:
            self._fade_job = self.parent.after(_STEP_MS, self._fade_in_step)
        else:
            self._fade_job = None
            self.window.geometry(f'+{self._final_bx}+{self._final_by}')

    # ── 淡出与隐藏 ────────────────────────────────────────────────────────

    def hide(self) -> None:
        """淡出气泡（外部调用）。若无气泡则静默返回。"""
        if self._hide_timer is not None:
            self.parent.after_cancel(self._hide_timer)
            self._hide_timer = None
        if self.window is None:
            self._cancel_fade()
            return
        self._cancel_fade()
        self._fade_start = time.monotonic()
        self._fade_out_step()

    def _fade_out_step(self) -> None:
        if self.window is None:
            self._fade_job = None
            return
        t = min(1.0, (time.monotonic() - self._fade_start) / (_FADE_OUT_MS / 1000.0))
        self.window.attributes('-alpha', 1.0 - t)
        if t < 1.0:
            self._fade_job = self.parent.after(_STEP_MS, self._fade_out_step)
        else:
            self._fade_job = None
            if self.window is not None:
                self.window.destroy()
                self.window = None

    # ── 内部工具 ──────────────────────────────────────────────────────────

    def _cancel_fade(self) -> None:
        if self._fade_job is not None:
            self.parent.after_cancel(self._fade_job)
            self._fade_job = None

    def _destroy_now(self) -> None:
        """立即取消所有动画并销毁窗口（在 show() 内部调用以替换旧气泡）"""
        self._cancel_fade()
        if self._hide_timer is not None:
            self.parent.after_cancel(self._hide_timer)
            self._hide_timer = None
        if self.window is not None:
            self.window.destroy()
            self.window = None
