"""
右键菜单模块
提供角色右键点击后弹出的上下文菜单（无系统托盘图标）

菜单布局
  ▶ Start Pomodoro   (idle / paused 时可用)
  ⏸ Pause            (running 时可用)
  ⏹ Stop             (非 idle 时可用)
  Duration ▶         (子菜单：15 / 25 / 45 / 60 分钟)
  ──────────────────
  Toggle Idle Animation
  ☑ Stretch Reminders
  ──────────────────
  Settings...
  About
  Exit
"""
import tkinter as tk
from typing import Callable


class ContextMenu:
    """
    右键上下文菜单，内置番茄钟控制区块与拉伸提醒开关。
    通过 update_pom_state() 按实际阶段启用 / 禁用番茄钟菜单项。
    """

    # 番茄钟菜单项的固定索引（相对于 self._menu）
    _IDX_START = 0
    _IDX_PAUSE = 1
    _IDX_STOP  = 2
    # index 3 = Duration submenu
    # index 4 = separator
    # index 5 = Toggle Idle Animation
    # index 6 = Stretch Reminders (checkbutton)
    # index 7 = separator
    # index 8 = Settings...
    # index 9 = About
    # index 10 = Exit

    def __init__(
        self,
        parent: tk.Tk,
        on_exit:              Callable,
        on_toggle_animation:  Callable,
        on_about:             Callable,
        on_pom_start:         Callable,
        on_pom_pause:         Callable,
        on_pom_stop:          Callable,
        on_pom_set_focus:     Callable,          # 接收 int（分钟数）
        on_stretch_toggle:    Callable,          # 接收 bool（启用状态）
        on_settings:          Callable,          # 打开设置窗口
    ):
        self._menu = tk.Menu(parent, tearoff=0, font=('Segoe UI', 10))

        # ── 番茄钟控制 ──
        self._menu.add_command(label='▶  Start Pomodoro', command=on_pom_start)
        self._menu.add_command(label='⏸  Pause',          command=on_pom_pause)
        self._menu.add_command(label='⏹  Stop',           command=on_pom_stop)

        # ── 时长子菜单 ──
        dur_menu = tk.Menu(self._menu, tearoff=0, font=('Segoe UI', 10))
        for mins in (15, 25, 45, 60):
            dur_menu.add_command(
                label=f'{mins} min',
                command=lambda m=mins: on_pom_set_focus(m),
            )
        self._menu.add_cascade(label='Duration  ▶', menu=dur_menu)

        self._menu.add_separator()

        # ── 通用控件 ──
        self._menu.add_command(label='Toggle Idle Animation', command=on_toggle_animation)

        # 拉伸提醒复选框（BooleanVar 持有勾选状态）
        self._stretch_var = tk.BooleanVar(value=False)
        self._menu.add_checkbutton(
            label='Stretch Reminders',
            variable=self._stretch_var,
            command=lambda: on_stretch_toggle(self._stretch_var.get()),
        )

        self._menu.add_separator()
        self._menu.add_command(label='Settings...', command=on_settings)
        self._menu.add_command(label='About',       command=on_about)
        self._menu.add_command(label='Exit',        command=on_exit)

        # 初始化番茄钟菜单为 idle 状态
        self.update_pom_state('idle', is_paused=False)

    def update_pom_state(self, phase: str, is_paused: bool) -> None:
        """
        根据番茄钟当前阶段与暂停状态更新菜单项的启用 / 禁用。
          idle                → Start 可用，Pause / Stop 禁用
          focus/break running → Pause 可用，Start / Stop 可用
          focus/break paused  → Start（恢复）可用，Pause 禁用，Stop 可用
        """
        is_idle    = (phase == 'idle')
        is_running = (not is_idle) and (not is_paused)

        start_state = tk.NORMAL if (is_idle or is_paused) else tk.DISABLED
        pause_state = tk.NORMAL if is_running              else tk.DISABLED
        stop_state  = tk.NORMAL if not is_idle             else tk.DISABLED

        self._menu.entryconfig(self._IDX_START, state=start_state)
        self._menu.entryconfig(self._IDX_PAUSE, state=pause_state)
        self._menu.entryconfig(self._IDX_STOP,  state=stop_state)

    def set_stretch_enabled(self, enabled: bool) -> None:
        """从外部同步复选框状态（设置窗口更改时调用）"""
        self._stretch_var.set(enabled)

    def show(self, x: int, y: int) -> None:
        """在屏幕绝对坐标 (x, y) 处弹出菜单"""
        try:
            self._menu.tk_popup(x, y)
        finally:
            self._menu.grab_release()
