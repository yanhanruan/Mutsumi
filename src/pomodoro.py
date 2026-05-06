"""
番茄钟计时器模块
PomodoroTimer 通过 tkinter after() 在主线程中驱动，无需额外线程。

状态机
  idle ──start()──► focus ──完成──► break ──完成──► focus ...
                         ◄──stop()────────────────────────
  任意运行状态可 pause()/start() 恢复，stop() 强制归零。

发布的事件（通过 events 总线）
  pomodoro_focus_start    (remaining: int)
  pomodoro_focus_complete ()
  pomodoro_break_start    (remaining: int)
  pomodoro_break_complete ()
"""
from __future__ import annotations
from PySide6.QtCore import QObject, QTimer
import events

class PomodoroTimer(QObject):
    IDLE  = 'idle'
    FOCUS = 'focus'
    BREAK = 'break'

    def __init__(self, parent: QObject = None, focus_mins: int = 25, break_mins: int = 5) -> None:
        super().__init__(parent)
        self.focus_mins = focus_mins
        self.break_mins = break_mins

        self._phase:    str  = self.IDLE
        self._paused:   bool = False
        self._remaining: int = 0

        self._timer = QTimer(self)
        self._timer.timeout.connect(self._tick)
        self._timer.setInterval(1000)

    @property
    def phase(self) -> str: return self._phase
    @property
    def is_paused(self) -> bool: return self._paused
    @property
    def remaining_str(self) -> str:
        m, s = divmod(max(0, self._remaining), 60)
        return f"{m:02d}:{s:02d}"

    def start(self) -> None:
        if self._phase == self.IDLE:
            self._begin_focus()
        elif self._paused:
            self._paused = False
            self._timer.start()

    def pause(self) -> None:
        if self._phase != self.IDLE and not self._paused:
            self._paused = True
            self._timer.stop()

    def stop(self) -> None:
        self._timer.stop()
        self._phase     = self.IDLE
        self._paused    = False
        self._remaining = 0

    def _begin_focus(self) -> None:
        self._phase     = self.FOCUS
        self._remaining = self.focus_mins * 60
        self._paused    = False
        self._timer.start()
        events.publish('pomodoro_focus_start', remaining=self._remaining)

    def _begin_break(self) -> None:
        self._phase     = self.BREAK
        self._remaining = self.break_mins * 60
        self._paused    = False
        self._timer.start()
        events.publish('pomodoro_break_start', remaining=self._remaining)

    def _tick(self) -> None:
        self._remaining -= 1
        if self._remaining <= 0:
            if self._phase == self.FOCUS:
                events.publish('pomodoro_focus_complete')
                self._begin_break()
            elif self._phase == self.BREAK:
                events.publish('pomodoro_break_complete')
                self._begin_focus()