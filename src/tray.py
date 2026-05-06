"""
Modern glassmorphism system tray integration.
Framework: PySide6
"""

from __future__ import annotations

from typing import Any, Callable, Optional

from PySide6.QtGui import QAction, QIcon
from PySide6.QtWidgets import QApplication, QMenu, QSystemTrayIcon
from PySide6.QtCore import QPoint

# 🔴 核心修复：引入我们新的 Theme 类，抛弃旧的 PALETTE
from ui_theme import Theme


class ContextMenu:
    def __init__(
        self,
        parent: Optional[Any] = None,  
        on_exit: Optional[Callable[[], None]] = None,
        on_toggle_animation: Optional[Callable[[], None]] = None,
        on_about: Optional[Callable[[], None]] = None,
        on_pom_start: Optional[Callable[[], None]] = None,
        on_pom_pause: Optional[Callable[[], None]] = None,
        on_pom_stop: Optional[Callable[[], None]] = None,
        on_pom_set_focus: Optional[Callable[[int], None]] = None,
        on_stretch_toggle: Optional[Callable[[bool], None]] = None,
        on_settings: Optional[Callable[[], None]] = None,
        icon_path: Optional[str] = None,
    ):
        on_exit = on_exit or (lambda: None)
        on_toggle_animation = on_toggle_animation or (lambda: None)
        on_about = on_about or (lambda: None)
        on_pom_start = on_pom_start or (lambda: None)
        on_pom_pause = on_pom_pause or (lambda: None)
        on_pom_stop = on_pom_stop or (lambda: None)
        on_pom_set_focus = on_pom_set_focus or (lambda _m: None)
        on_stretch_toggle = on_stretch_toggle or (lambda _e: None)
        on_settings = on_settings or (lambda: None)

        self.tray = QSystemTrayIcon(parent)
        icon = QIcon(icon_path) if icon_path else QApplication.style().standardIcon(QApplication.style().SP_ComputerIcon)
        self.tray.setIcon(icon)
        self.tray.setToolTip("Mutsumi - Premium Desktop Pet")

        # 🔴 核心修复：使用新的 Theme 变量重写托盘样式
        self.menu = QMenu()
        self.menu.setStyleSheet(
            f"""
            QMenu {{
                background: rgba(28, 30, 32, 0.85); /* 半透明暗色背景 */
                color: #F3F6F3;
                border: 1px solid rgba(255, 255, 255, 0.35);
                border-radius: 8px;
                padding: 6px;
                font-family: 'Segoe UI', sans-serif;
                font-size: 10pt;
            }}
            QMenu::item {{
                padding: 6px 20px 6px 12px;
                border-radius: 4px;
                margin: 2px;
                background: transparent;
            }}
            QMenu::item:selected {{
                background: {Theme.PRIMARY}; /* 鼠尾草绿的高级感悬浮色 */
                color: white;
            }}
            QMenu::separator {{
                height: 1px;
                background: rgba(255, 255, 255, 0.35);
                margin: 4px 10px;
            }}
            """
        )

        self.start_action = QAction("Start Pomodoro", self.menu)
        self.pause_action = QAction("Pause", self.menu)
        self.stop_action = QAction("Stop", self.menu)
        self.start_action.triggered.connect(on_pom_start)
        self.pause_action.triggered.connect(on_pom_pause)
        self.stop_action.triggered.connect(on_pom_stop)
        self.menu.addAction(self.start_action)
        self.menu.addAction(self.pause_action)
        self.menu.addAction(self.stop_action)

        duration_menu = self.menu.addMenu("Duration")
        for mins in (15, 25, 45, 60):
            action = QAction(f"{mins} min", duration_menu)
            action.triggered.connect(lambda _checked=False, m=mins: on_pom_set_focus(m))
            duration_menu.addAction(action)

        self.menu.addSeparator()

        toggle_action = QAction("Toggle Idle Animation", self.menu)
        toggle_action.triggered.connect(on_toggle_animation)
        self.menu.addAction(toggle_action)

        self.stretch_action = QAction("Stretch Reminders", self.menu)
        self.stretch_action.setCheckable(True)
        self.stretch_action.toggled.connect(on_stretch_toggle)
        self.menu.addAction(self.stretch_action)

        self.menu.addSeparator()

        settings_action = QAction("Settings", self.menu)
        settings_action.triggered.connect(on_settings)
        self.menu.addAction(settings_action)

        about_action = QAction("About", self.menu)
        about_action.triggered.connect(on_about)
        self.menu.addAction(about_action)

        self.menu.addSeparator()
        
        exit_action = QAction("Exit", self.menu)
        exit_action.triggered.connect(on_exit)
        self.menu.addAction(exit_action)

        self.tray.setContextMenu(self.menu)
        self.tray.show()
        
        self.update_pom_state("idle", is_paused=False)

    def update_pom_state(self, phase: str, is_paused: bool) -> None:
        is_idle = phase == "idle"
        is_running = (not is_idle) and (not is_paused)
        self.start_action.setEnabled(is_idle or is_paused)
        self.pause_action.setEnabled(is_running)
        self.stop_action.setEnabled(not is_idle)

    def set_stretch_enabled(self, enabled: bool) -> None:
        self.stretch_action.setChecked(enabled)

    def show(self, x: int, y: int) -> None:
        self.menu.popup(QPoint(x, y))