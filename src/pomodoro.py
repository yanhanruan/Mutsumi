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
import tkinter as tk
from typing import Optional

import events


class PomodoroTimer:
    """番茄钟计时器，不持有任何 UI 引用（root 仅用于 after() 调度）"""

    # ── 阶段常量 ──
    IDLE  = 'idle'
    FOCUS = 'focus'
    BREAK = 'break'

    def __init__(
        self,
        root: tk.Tk,
        focus_mins: int = 25,
        break_mins: int = 5,
    ) -> None:
        self.root       = root
        self.focus_mins = focus_mins
        self.break_mins = break_mins

        self._phase:    str  = self.IDLE
        self._paused:   bool = False
        self._remaining: int = 0          # 当前阶段剩余秒数
        self._tick_job: Optional[str] = None

    # ── 只读属性 ──────────────────────────────────────────────────────────

    @property
    def phase(self) -> str:
        """当前阶段标识：'idle' | 'focus' | 'break'"""
        return self._phase

    @property
    def is_running(self) -> bool:
        """计时器正在倒计时（非暂停、非空闲）"""
        return self._phase != self.IDLE and not self._paused

    @property
    def is_paused(self) -> bool:
        """处于暂停态（有阶段但未在倒计时）"""
        return self._paused and self._phase != self.IDLE

    @property
    def is_idle(self) -> bool:
        return self._phase == self.IDLE

    @property
    def remaining_seconds(self) -> int:
        return max(0, self._remaining)

    @property
    def remaining_str(self) -> str:
        """格式化剩余时间为 MM:SS"""
        m, s = divmod(self.remaining_seconds, 60)
        return f'{m:02d}:{s:02d}'

    # ── 配置 ──────────────────────────────────────────────────────────────

    def set_focus_mins(self, mins: int) -> None:
        """更新专注时长；对当前阶段无影响，下一个周期生效"""
        self.focus_mins = mins

    def set_break_mins(self, mins: int) -> None:
        self.break_mins = mins

    # ── 公开控制接口 ──────────────────────────────────────────────────────

    def start(self) -> None:
        """
        idle  → 开始新的专注阶段
        paused → 恢复当前阶段
        running → 无操作
        """
        if self._phase == self.IDLE:
            self._begin_focus()
        elif self._paused:
            self._paused = False
            self._schedule_tick()

    def pause(self) -> None:
        """暂停正在运行的阶段（已暂停或空闲时无操作）"""
        if self.is_running:
            self._paused = True
            self._cancel_tick()

    def stop(self) -> None:
        """停止并完全重置（不发布任何事件）"""
        self._cancel_tick()
        self._phase     = self.IDLE
        self._paused    = False
        self._remaining = 0

    # ── 内部状态机 ────────────────────────────────────────────────────────

    def _begin_focus(self) -> None:
        self._phase     = self.FOCUS
        self._remaining = self.focus_mins * 60
        self._paused    = False
        self._schedule_tick()
        events.publish('pomodoro_focus_start', remaining=self._remaining)

    def _begin_break(self) -> None:
        self._phase     = self.BREAK
        self._remaining = self.break_mins * 60
        self._paused    = False
        self._schedule_tick()
        events.publish('pomodoro_break_start', remaining=self._remaining)

    def _schedule_tick(self) -> None:
        self._tick_job = self.root.after(1000, self._tick)

    def _cancel_tick(self) -> None:
        if self._tick_job is not None:
            self.root.after_cancel(self._tick_job)
            self._tick_job = None

    def _tick(self) -> None:
        """每秒触发：递减计数，边界处切换阶段（自动循环）"""
        self._tick_job   = None
        self._remaining -= 1

        if self._remaining <= 0:
            if self._phase == self.FOCUS:
                events.publish('pomodoro_focus_complete')
                self._begin_break()     # 自动进入休息
            else:
                events.publish('pomodoro_break_complete')
                self._begin_focus()     # 自动开始下一个专注周期
        else:
            self._schedule_tick()
