"""
设置数据类与设置窗口
框架：PySide6
"""
from __future__ import annotations
from typing import Callable, Optional
from PySide6.QtWidgets import (QWidget, QVBoxLayout, QHBoxLayout, QLabel, QFrame,
                               QCheckBox, QSlider, QPushButton, QMessageBox)
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
        self.resize(350, 480)

        self.settings   = settings_obj
        self.on_change  = on_change_callback
        self.on_reset   = on_reset_callback
        self.drag_pos   = None

        self._init_ui()

    # ── UI construction ───────────────────────────────────────────────────

    def _init_ui(self):
        root = QVBoxLayout(self)
        root.setContentsMargins(10, 10, 10, 10)

        self.glass_panel = QFrame(self)
        self.glass_panel.setObjectName("GlassPanel")
        self.glass_panel.setStyleSheet(Theme.GLASS_STYLE)
        panel = QVBoxLayout(self.glass_panel)
        panel.setSpacing(14)

        title = QLabel("✦ Mutsumi Settings")
        title.setStyleSheet(f"font-size: 16px; font-weight: bold; color: {Theme.PRIMARY};")
        panel.addWidget(title)

        # ── Personality ──
        panel.addWidget(self._section_label("Personality"))

        self.chatter_cb = QCheckBox("Idle chatter")
        self.chatter_cb.setChecked(self.settings.idle_chatter_on)
        self.chatter_cb.toggled.connect(self._on_chatter_toggle)
        panel.addWidget(self.chatter_cb)

        self.chatter_slider, chatter_row = self._labeled_slider(
            "Frequency (min):", 1, 30, self.settings.idle_chatter_freq_mins)
        self.chatter_slider.valueChanged.connect(self._on_chatter_freq)
        panel.addLayout(chatter_row)

        # ── Pomodoro ──
        panel.addWidget(self._section_label("Pomodoro"))

        self.focus_slider, focus_row = self._labeled_slider(
            "Focus length (min):", 5, 60, self.settings.pom_focus_mins)
        self.focus_slider.valueChanged.connect(self._on_focus_mins)
        panel.addLayout(focus_row)

        self.break_slider, break_row = self._labeled_slider(
            "Break length (min):", 1, 30, self.settings.pom_break_mins)
        self.break_slider.valueChanged.connect(self._on_break_mins)
        panel.addLayout(break_row)

        # ── Wellness ──
        panel.addWidget(self._section_label("Wellness"))

        self.stretch_cb = QCheckBox("Stretch reminders")
        self.stretch_cb.setChecked(self.settings.stretch_reminders_on)
        self.stretch_cb.toggled.connect(self._on_stretch_toggle)
        panel.addWidget(self.stretch_cb)

        self.volume_slider, volume_row = self._labeled_slider(
            "Master volume:", 0, 100, self.settings.master_volume)
        self.volume_slider.valueChanged.connect(self._on_volume)
        panel.addLayout(volume_row)

        panel.addStretch()

        # ── Buttons ──
        btn_row = QHBoxLayout()
        reset_btn = QPushButton("Reset State")
        reset_btn.setStyleSheet("color: #999999; border-color: #DDDDDD;")
        reset_btn.clicked.connect(self._handle_reset)
        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.hide)
        btn_row.addWidget(reset_btn)
        btn_row.addStretch()
        btn_row.addWidget(close_btn)
        panel.addLayout(btn_row)

        root.addWidget(self.glass_panel)

    @staticmethod
    def _section_label(text: str) -> QLabel:
        lbl = QLabel(text)
        lbl.setStyleSheet("font-weight: bold; color: #555555; margin-top: 4px;")
        return lbl

    @staticmethod
    def _labeled_slider(label: str, lo: int, hi: int, value: int):
        slider = QSlider(Qt.Orientation.Horizontal)
        slider.setRange(lo, hi)
        slider.setValue(value)
        val_lbl = QLabel(str(value))
        val_lbl.setFixedWidth(28)
        slider.valueChanged.connect(lambda v: val_lbl.setText(str(v)))
        row = QHBoxLayout()
        row.addWidget(QLabel(label))
        row.addWidget(slider)
        row.addWidget(val_lbl)
        return slider, row

    # ── Callbacks ─────────────────────────────────────────────────────────

    def _on_chatter_toggle(self, checked: bool):
        self.settings.idle_chatter_on = checked
        self.on_change(self.settings)

    def _on_chatter_freq(self, value: int):
        self.settings.idle_chatter_freq_mins = value
        self.on_change(self.settings)

    def _on_focus_mins(self, value: int):
        self.settings.pom_focus_mins = value
        self.on_change(self.settings)

    def _on_break_mins(self, value: int):
        self.settings.pom_break_mins = value
        self.on_change(self.settings)

    def _on_stretch_toggle(self, checked: bool):
        self.settings.stretch_reminders_on = checked
        self.on_change(self.settings)

    def _on_volume(self, value: int):
        self.settings.master_volume = value
        self.on_change(self.settings)

    def _handle_reset(self):
        reply = QMessageBox.question(
            self, 'Reset State',
            'Reset energy, affection, and mood to defaults?\nThis cannot be undone.')
        if reply == QMessageBox.StandardButton.Yes:
            self.on_reset()

    # ── Draggable window ──────────────────────────────────────────────────

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.drag_pos = event.globalPos() - self.frameGeometry().topLeft()
            event.accept()

    def mouseMoveEvent(self, event):
        if event.buttons() == Qt.MouseButton.LeftButton and self.drag_pos:
            self.move(event.globalPos() - self.drag_pos)
            event.accept()
