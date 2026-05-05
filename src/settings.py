"""
设置数据类与设置窗口
Settings 数据类在整个程序生命周期内以引用方式共享；
SettingsWindow 通过修改该数据类并调用 on_change 回调实现实时应用。
"""
from __future__ import annotations
import tkinter as tk
import tkinter.messagebox
from typing import Callable, Optional


class Settings:
    """程序运行时设置（可序列化为 dict）"""

    def __init__(
        self,
        idle_chatter_on:       bool = True,
        idle_chatter_freq_mins: int = 3,
        pom_focus_mins:         int = 25,
        pom_break_mins:         int = 5,
        stretch_reminders_on:  bool = False,
        master_volume:          int = 50,
    ) -> None:
        self.idle_chatter_on        = idle_chatter_on
        self.idle_chatter_freq_mins = idle_chatter_freq_mins
        self.pom_focus_mins         = pom_focus_mins
        self.pom_break_mins         = pom_break_mins
        self.stretch_reminders_on   = stretch_reminders_on
        self.master_volume          = master_volume

    def to_dict(self) -> dict:
        return {
            'idle_chatter_on':        self.idle_chatter_on,
            'idle_chatter_freq_mins': self.idle_chatter_freq_mins,
            'pom_focus_mins':         self.pom_focus_mins,
            'pom_break_mins':         self.pom_break_mins,
            'stretch_reminders_on':   self.stretch_reminders_on,
            'master_volume':          self.master_volume,
        }

    @classmethod
    def from_dict(cls, d: dict) -> 'Settings':
        return cls(
            idle_chatter_on        = bool(d.get('idle_chatter_on', True)),
            idle_chatter_freq_mins = int(d.get('idle_chatter_freq_mins', 3)),
            pom_focus_mins         = int(d.get('pom_focus_mins', 25)),
            pom_break_mins         = int(d.get('pom_break_mins', 5)),
            stretch_reminders_on   = bool(d.get('stretch_reminders_on', False)),
            master_volume          = int(d.get('master_volume', 50)),
        )


class SettingsWindow:
    """
    小型 Tk 设置窗口（单例：已存在时 show() 只 lift 不重建）。
    用户更改控件值时立即调用 on_change(settings)，应用无需重启。
    """

    def __init__(
        self,
        parent:         tk.Tk,
        settings:       Settings,
        on_change:      Callable[[Settings], None],
        on_reset_state: Callable[[], None],
    ) -> None:
        self._parent        = parent
        self._settings      = settings
        self._on_change     = on_change
        self._on_reset_state = on_reset_state
        self._win:   Optional[tk.Toplevel] = None

    # ── 公开接口 ──────────────────────────────────────────────────────────

    def show(self) -> None:
        if self._win is not None and self._win.winfo_exists():
            self._win.lift()
            self._win.focus_force()
            return
        self._build()

    # ── 窗口构建 ──────────────────────────────────────────────────────────

    def _build(self) -> None:
        win = tk.Toplevel(self._parent)
        win.title('Mutsumi — Settings')
        win.resizable(False, False)
        win.attributes('-topmost', True)
        win.protocol('WM_DELETE_WINDOW', self._on_close)
        self._win = win

        pad = {'padx': 10, 'pady': 4}

        # ── PERSONALITY ──────────────────────────────────────────────────
        pf = tk.LabelFrame(win, text='Personality', font=('Segoe UI', 10, 'bold'),
                           padx=8, pady=6)
        pf.pack(fill='x', padx=12, pady=(12, 4))

        self._chatter_var = tk.BooleanVar(value=self._settings.idle_chatter_on)
        tk.Checkbutton(
            pf, text='Idle chatter',
            variable=self._chatter_var,
            font=('Segoe UI', 10),
            command=self._on_changed,
        ).grid(row=0, column=0, columnspan=2, sticky='w', **pad)

        tk.Label(pf, text='Chatter frequency (min):',
                 font=('Segoe UI', 10)).grid(row=1, column=0, sticky='w', **pad)
        self._freq_var = tk.IntVar(value=self._settings.idle_chatter_freq_mins)
        freq_frame = tk.Frame(pf)
        freq_frame.grid(row=1, column=1, sticky='w', **pad)
        self._freq_scale = tk.Scale(
            freq_frame, from_=1, to=10, orient='horizontal',
            variable=self._freq_var, length=140,
            font=('Segoe UI', 9),
            command=lambda _: self._on_changed(),
        )
        self._freq_scale.pack(side='left')

        # ── POMODORO ─────────────────────────────────────────────────────
        pmf = tk.LabelFrame(win, text='Pomodoro', font=('Segoe UI', 10, 'bold'),
                            padx=8, pady=6)
        pmf.pack(fill='x', padx=12, pady=4)

        tk.Label(pmf, text='Focus length (min):',
                 font=('Segoe UI', 10)).grid(row=0, column=0, sticky='w', **pad)
        self._focus_var = tk.IntVar(value=self._settings.pom_focus_mins)
        tk.Spinbox(
            pmf, from_=1, to=120, textvariable=self._focus_var, width=5,
            font=('Segoe UI', 10), command=self._on_changed,
        ).grid(row=0, column=1, sticky='w', **pad)

        tk.Label(pmf, text='Break length (min):',
                 font=('Segoe UI', 10)).grid(row=1, column=0, sticky='w', **pad)
        self._break_var = tk.IntVar(value=self._settings.pom_break_mins)
        tk.Spinbox(
            pmf, from_=1, to=60, textvariable=self._break_var, width=5,
            font=('Segoe UI', 10), command=self._on_changed,
        ).grid(row=1, column=1, sticky='w', **pad)

        # ── WELLNESS ─────────────────────────────────────────────────────
        wf = tk.LabelFrame(win, text='Wellness', font=('Segoe UI', 10, 'bold'),
                           padx=8, pady=6)
        wf.pack(fill='x', padx=12, pady=4)

        self._stretch_var = tk.BooleanVar(value=self._settings.stretch_reminders_on)
        tk.Checkbutton(
            wf, text='Stretch reminders (every 45 min)',
            variable=self._stretch_var,
            font=('Segoe UI', 10),
            command=self._on_changed,
        ).grid(row=0, column=0, columnspan=2, sticky='w', **pad)

        tk.Label(wf, text='Master volume:',
                 font=('Segoe UI', 10)).grid(row=1, column=0, sticky='w', **pad)
        self._vol_var = tk.IntVar(value=self._settings.master_volume)
        tk.Scale(
            wf, from_=0, to=100, orient='horizontal',
            variable=self._vol_var, length=140,
            font=('Segoe UI', 9),
            command=lambda _: self._on_changed(),
        ).grid(row=1, column=1, sticky='w', **pad)

        # ── RESET / CLOSE ─────────────────────────────────────────────────
        btn_frame = tk.Frame(win)
        btn_frame.pack(fill='x', padx=12, pady=(4, 12))

        tk.Button(
            btn_frame, text='Reset State',
            font=('Segoe UI', 10),
            fg='#B71C1C',
            command=self._on_reset_clicked,
        ).pack(side='left', padx=(0, 8))

        tk.Button(
            btn_frame, text='Close',
            font=('Segoe UI', 10),
            command=self._on_close,
        ).pack(side='right')

        # Center on screen
        win.update_idletasks()
        sw = win.winfo_screenwidth()
        sh = win.winfo_screenheight()
        w  = win.winfo_reqwidth()
        h  = win.winfo_reqheight()
        win.geometry(f'+{(sw - w) // 2}+{(sh - h) // 2}')

    # ── 回调 ──────────────────────────────────────────────────────────────

    def _on_changed(self) -> None:
        """读取控件值，更新 Settings 对象，通知调用方。"""
        s = self._settings
        s.idle_chatter_on        = self._chatter_var.get()
        s.idle_chatter_freq_mins = self._freq_var.get()
        s.pom_focus_mins         = self._focus_var.get()
        s.pom_break_mins         = self._break_var.get()
        s.stretch_reminders_on   = self._stretch_var.get()
        s.master_volume          = self._vol_var.get()
        self._on_change(s)

    def _on_reset_clicked(self) -> None:
        """重置宠物状态前二次确认"""
        if self._win is None:
            return
        if tk.messagebox.askyesno(
            'Reset State',
            'Reset energy, affection, and mood to defaults?\nThis cannot be undone.',
            parent=self._win,
        ):
            self._on_reset_state()

    def _on_close(self) -> None:
        if self._win is not None:
            self._win.destroy()
            self._win = None

    # ── 外部同步 ──────────────────────────────────────────────────────────

    def sync_stretch(self, enabled: bool) -> None:
        """当菜单的 Stretch Reminders 复选框改变时，同步到设置窗口的控件。"""
        if self._win is not None and self._win.winfo_exists():
            self._stretch_var.set(enabled)
