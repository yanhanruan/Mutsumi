"""
设置数据类与设置窗口
框架：PySide6
"""
from __future__ import annotations
from typing import Callable, Optional
from PySide6.QtWidgets import (QWidget, QVBoxLayout, QLabel, QFrame, 
                               QCheckBox, QSlider, QPushButton, QHBoxLayout, QMessageBox)
from PySide6.QtCore import Qt
from ui_theme import Theme

class Settings:
    """程序运行时设置"""
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

class SettingsWindow(QWidget):
    def __init__(self, settings_obj: Settings, on_change_callback: Callable[[Settings], None], on_reset_callback: Callable[[], None]):
        super().__init__()
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.resize(350, 450)
        
        self.settings = settings_obj
        self.on_change = on_change_callback
        self.on_reset = on_reset_callback
        self.drag_pos = None

        self.init_ui()

    def init_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 10, 10, 10)

        self.glass_panel = QFrame(self)
        self.glass_panel.setStyleSheet(Theme.GLASS_STYLE)
        panel_layout = QVBoxLayout(self.glass_panel)

        title = QLabel("✦ Mutsumi Settings")
        title.setStyleSheet(f"font-size: 16px; font-weight: bold; color: {Theme.PRIMARY};")
        panel_layout.addWidget(title)

        # 此处可根据你的具体需求添加 QCheckBox 和 QSlider 
        # 当滑动或点击时，更新 self.settings 并调用 self.on_change(self.settings)
        # (篇幅原因简化了控件，你可以参考之前提供的 settings.py UI 布局将其丰富)

        btn_layout = QHBoxLayout()
        reset_btn = QPushButton("Reset State")
        reset_btn.clicked.connect(self._handle_reset)
        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.hide)
        
        btn_layout.addWidget(reset_btn)
        btn_layout.addStretch()
        btn_layout.addWidget(close_btn)
        panel_layout.addLayout(btn_layout)
        layout.addWidget(self.glass_panel)

    def _handle_reset(self):
        reply = QMessageBox.question(self, 'Reset State', 'Reset energy, affection, and mood to defaults?\nThis cannot be undone.')
        if reply == QMessageBox.StandardButton.Yes:
            self.on_reset()

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.drag_pos = event.globalPos() - self.frameGeometry().topLeft()
            event.accept()

    def mouseMoveEvent(self, event):
        if event.buttons() == Qt.MouseButton.LeftButton and self.drag_pos:
            self.move(event.globalPos() - self.drag_pos)
            event.accept()