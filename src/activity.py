"""
用户活动监控模块

每 30 秒采样一次系统鼠标坐标，通过位移比较推断用户是否在场。
完全由 tkinter after() 驱动，无额外线程。

坐标获取优先级
  1. ctypes.windll.user32.GetCursorPos（Windows 内置，无需额外依赖）
  2. pyautogui.position()（跨平台备选，需安装 pyautogui）
  3. (0, 0) 兜底（两者均不可用时静默降级）

发布的事件（通过 events 总线）
  user_idle        鼠标静止超过 20 分钟时发布一次
  user_returned    从 idle 恢复活动时发布一次
  stretch_reminder 连续活动满 45 分钟时发布（需启用拉伸提醒）
"""
from __future__ import annotations
import sys
import tkinter as tk
from datetime import datetime
from typing import Optional, Tuple

import events


# ── 时间常量 ──
_POLL_MS             = 30_000   # 采样间隔：30 秒
_IDLE_THRESHOLD_SECS = 1200     # 判定离开：20 分钟无位移
_STRETCH_SECS        = 2700     # 拉伸提醒：连续活动 45 分钟


def _get_mouse_pos() -> Tuple[int, int]:
    """读取当前鼠标屏幕坐标；失败时静默返回 (0, 0)"""
    if sys.platform == 'win32':
        try:
            import ctypes

            class _POINT(ctypes.Structure):
                _fields_ = [('x', ctypes.c_long), ('y', ctypes.c_long)]

            pt = _POINT()
            ctypes.windll.user32.GetCursorPos(ctypes.byref(pt))
            return (int(pt.x), int(pt.y))
        except Exception:
            pass
    try:
        import pyautogui  # type: ignore
        p = pyautogui.position()
        return (int(p.x), int(p.y))
    except Exception:
        return (0, 0)


class ActivityMonitor:
    """
    系统级用户活动监控器。
    实例化后调用 start() 开始轮询；退出时调用 stop() 取消定时器。
    """

    def __init__(self, root: tk.Tk) -> None:
        self.root = root

        self._last_pos:           Tuple[int, int] = _get_mouse_pos()
        self._last_move_at:       datetime        = datetime.now()
        self._activity_start_at:  datetime        = datetime.now()

        self._is_idle:        bool          = False
        self._stretch_enabled: bool         = False
        self._poll_job:       Optional[str] = None

    # ── 公开接口 ──────────────────────────────────────────────────────────

    def start(self) -> None:
        """启动轮询循环"""
        if self._poll_job is None:
            self._schedule_poll()

    def stop(self) -> None:
        """停止轮询（应用退出时调用）"""
        if self._poll_job is not None:
            self.root.after_cancel(self._poll_job)
            self._poll_job = None

    def set_stretch_reminders(self, enabled: bool) -> None:
        """启用 / 禁用拉伸提醒；启用时从当前时刻重新开始连续活动计时"""
        self._stretch_enabled = enabled
        if enabled:
            self._activity_start_at = datetime.now()
        print(f'[Activity] Stretch reminders {"enabled" if enabled else "disabled"}.')

    # ── 内部逻辑 ──────────────────────────────────────────────────────────

    def _schedule_poll(self) -> None:
        self._poll_job = self.root.after(_POLL_MS, self._poll)

    def _poll(self) -> None:
        self._poll_job = None
        now = datetime.now()
        pos = _get_mouse_pos()
        moved = (pos != self._last_pos)

        if moved:
            self._last_pos     = pos
            self._last_move_at = now

            if self._is_idle:
                # 从离开状态恢复
                self._is_idle          = False
                self._activity_start_at = now
                events.publish('user_returned')
                print('[Activity] User returned.')
        else:
            idle_secs = (now - self._last_move_at).total_seconds()
            if not self._is_idle and idle_secs >= _IDLE_THRESHOLD_SECS:
                self._is_idle = True
                events.publish('user_idle')
                print(f'[Activity] User idle ({idle_secs / 60:.0f} min).')

        # 拉伸提醒：用户在场 + 功能已启用时检查连续活动时长
        if self._stretch_enabled and not self._is_idle:
            active_secs = (now - self._activity_start_at).total_seconds()
            if active_secs >= _STRETCH_SECS:
                self._activity_start_at = now   # 重置，等待下一个 45 分钟
                events.publish('stretch_reminder')
                print(f'[Activity] Stretch reminder ({active_secs / 60:.0f} min active).')

        self._schedule_poll()
