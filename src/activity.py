"""
用户活动监控模块
框架：PySide6

每 30 秒采样一次系统全局鼠标坐标，推断用户是否在场。
使用 PySide6 原生的 QTimer 和 QCursor，无需外部 ctypes 或 pyautogui 依赖。
"""
from __future__ import annotations
from datetime import datetime
from PySide6.QtCore import QObject, QTimer
from PySide6.QtGui import QCursor

import events

_POLL_MS             = 30_000   # 采样间隔：30 秒
_IDLE_THRESHOLD_SECS = 1200     # 判定离开：20 分钟无位移
_STRETCH_SECS        = 2700     # 拉伸提醒：连续活动 45 分钟

class ActivityMonitor(QObject):
    def __init__(self, parent: QObject = None, stretch_enabled: bool = False) -> None:
        super().__init__(parent)
        self._stretch_enabled   = stretch_enabled
        self._is_idle           = False
        
        # PySide6 原生获取全局鼠标坐标
        self._last_pos          = QCursor.pos()
        self._last_move_at      = datetime.now()
        self._activity_start_at = datetime.now()

        # 使用 QTimer 替代 root.after
        self._timer = QTimer(self)
        self._timer.timeout.connect(self._poll)
        self._timer.start(_POLL_MS)

    def set_stretch_enabled(self, enabled: bool) -> None:
        self._stretch_enabled = enabled
        print(f'[Activity] Stretch reminders {"enabled" if enabled else "disabled"}.')

    def _poll(self) -> None:
        now = datetime.now()
        pos = QCursor.pos()
        moved = (pos != self._last_pos)

        if moved:
            self._last_pos     = pos
            self._last_move_at = now

            if self._is_idle:
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

        if self._stretch_enabled and not self._is_idle:
            active_secs = (now - self._activity_start_at).total_seconds()
            if active_secs >= _STRETCH_SECS:
                self._activity_start_at = now
                events.publish('stretch_reminder')