"""
桌面宠物主窗口模块 (基于 PySide6 重构 - Glassmorphism 版本)
负责：窗口创建与透明化、动画状态机、鼠标交互、PetState 驱动、气泡与菜单调用
"""
from __future__ import annotations
import math
import os
import sys
import random
import time
from datetime import datetime
from typing import Dict, List, Optional, Tuple

from PySide6.QtWidgets import QWidget, QLabel, QMessageBox, QApplication
from PySide6.QtCore import Qt, QTimer, QPoint
from PySide6.QtGui import QPixmap

from PIL.ImageQt import ImageQt

import events
from placeholder  import create_placeholder_character
from animator     import (Animation,
                           generate_click_frames,
                           generate_drag_frames, generate_fly_frames,
                           generate_dizzy_frames, generate_squash_frames)
from activity     import ActivityMonitor
from bubble       import ChatBubble  # 确保这里导入的名字和你 bubble.py 里新类的名字一致
from persistence  import Persistence
from pomodoro     import PomodoroTimer
from settings     import Settings, SettingsWindow
from state        import PetState, Mood
from tray         import ContextMenu
from zparticles   import ZParticles


class DesktopPet(QWidget):
    def __init__(self):
        super().__init__()
        
        # ── 1. 窗口基础配置 (无边框 + 顶部悬浮 + 背景透明) ──
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | 
                            Qt.WindowType.WindowStaysOnTopHint | 
                            Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)

        # ── 2. 初始化核心状态与逻辑模块 ──
        self._anim = 'idle'
        self._pet_state = PetState()
        self._pom = PomodoroTimer(self)
        self._settings = Settings()
        self._activity = ActivityMonitor(self, stretch_enabled=self._settings.stretch_reminders_on)
        self._persistence = Persistence(self)

        # ── 3. 图像渲染载体 ──
        self.pet_label = QLabel(self)
        self.pet_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        self._animations = self._load_all_animations()
        self._current_frame_generator = iter(self._animations['idle'].frames)

        # Size window to first idle frame immediately (instead of waiting for first tick)
        if self._animations['idle'].frames:
            _first = self._animations['idle'].frames[0]
            self.resize(_first.width(), _first.height())
            self.pet_label.resize(_first.width(), _first.height())

        # ── 4. 初始化附属 UI 组件 ──
        self.chat_bubble = ChatBubble()
        self.settings_window = SettingsWindow(self._settings, self._on_settings_change, self._on_reset)
        self.z_particles = ZParticles(self)
        
        self.tray = ContextMenu(
            parent=self,
            on_exit=QApplication.instance().quit,
            on_about=self._on_about,
            on_settings=self.settings_window.show,
            on_pom_start=self._pom.start,
            on_pom_pause=self._pom.pause,
            on_pom_stop=self._pom.stop,
        )

        # ── 5. 替换原本的 root.after 定时器 ──
        # 主动画循环定时器 (替代 root.after(100, update))
        self.anim_timer = QTimer(self)
        self.anim_timer.timeout.connect(self._animate_step)
        self.anim_timer.start(self._animations['idle'].interval_ms)

        # 番茄钟 Badge 刷新定时器
        self.badge_timer = QTimer(self)
        self.badge_timer.timeout.connect(self._update_badge)

        # 宠物状态 tick 定时器（每 5 秒驱动能量/亲密度衰减和心情重算）
        self.state_timer = QTimer(self)
        self.state_timer.timeout.connect(self._pet_state.tick)
        self.state_timer.start(5000)

        # 物理拖拽变量
        self._drag_start_pos = None
        self._drag_start_time: Optional[float] = None

        # 番茄钟事件订阅 — 开始时启动 badge 刷新
        events.subscribe('pomodoro_focus_start', lambda **_: self.badge_timer.start(1000))
        events.subscribe('pomodoro_break_start', lambda **_: self.badge_timer.start(1000))

    # ── 动画加载 ──────────────────────────────────────────────────────────
    @staticmethod
    def _idle_frames_dir() -> str:
        src_dir = os.path.dirname(os.path.abspath(__file__))
        return os.path.join(os.path.dirname(src_dir), 'assets', 'idle')

    _IDLE_DISPLAY_HEIGHT = 200  # px; change to scale the pet up or down

    @staticmethod
    def _load_idle_from_dir(fps: float = 24.0, loop: bool = True) -> Animation:
        frames_dir = DesktopPet._idle_frames_dir()
        paths = sorted(
            f for f in (os.path.join(frames_dir, n) for n in os.listdir(frames_dir))
            if f.lower().endswith('.png')
        )
        h = DesktopPet._IDLE_DISPLAY_HEIGHT
        frames = [
            QPixmap(p).scaledToHeight(h, Qt.TransformationMode.SmoothTransformation)
            for p in paths
        ]
        return Animation(frames, fps=fps, loop=loop)

    @staticmethod
    def _pil_anim(pil_frames: list, fps: float, loop: bool) -> Animation:
        pixmaps = [QPixmap.fromImage(ImageQt(f)) for f in pil_frames]
        return Animation(pixmaps, fps=fps, loop=loop)

    def _load_all_animations(self) -> dict:
        char = create_placeholder_character()
        squash_pil, rebound_pil = generate_squash_frames(char)
        return {
            'idle':    self._load_idle_from_dir(fps=24.0, loop=True),
            'click':   self._pil_anim(generate_click_frames(char),  fps=12, loop=False),
            'drag':    self._pil_anim(generate_drag_frames(char),   fps=10, loop=True),
            'fly':     self._pil_anim(generate_fly_frames(char),    fps=16, loop=True),
            'dizzy':   self._pil_anim(generate_dizzy_frames(char),  fps=10, loop=False),
            'squash':  self._pil_anim(squash_pil,                   fps=12, loop=False),
            'rebound': self._pil_anim(rebound_pil,                  fps=12, loop=False),
        }

    # ── 动画渲染核心 ──────────────────────────────────────────────────────
    def _animate_step(self):
        try:
            pixmap: QPixmap = next(self._current_frame_generator)
            self.pet_label.setPixmap(pixmap)
            self.pet_label.resize(pixmap.width(), pixmap.height())
            self.resize(pixmap.width(), pixmap.height())
        except StopIteration:
            self._set_anim('idle')

    def _set_anim(self, anim_name: str):
        if anim_name not in self._animations:
            return
        
        self._anim = anim_name
        anim_obj = self._animations[anim_name]
        
        # 重新生成迭代器，确保从第一帧开始播放
        self._current_frame_generator = iter(anim_obj.frames)
        
        # 根据该动画设定的 fps 调整定时器间隔 (1000ms / fps)
        # 如果 Animation 对象里有 interval_ms 属性则直接使用
        if hasattr(anim_obj, 'interval_ms'):
            self.anim_timer.setInterval(anim_obj.interval_ms)
        else:
            self.anim_timer.setInterval(int(1000 / anim_obj.fps))
            
    # ── 鼠标交互 (替代 bind 事件) ─────────────────────────────────────────
    _CLICK_PHRASES = [
        "I'm not a toy, you know!",
        "Ahh~ be gentle!",
        "What do you want~?",
        "Eek! That tickles!",
        "H-Hey! Stop that!",
    ]

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_start_pos = event.globalPos() - self.frameGeometry().topLeft()
            self._drag_start_time = time.monotonic()
            self._pet_state.on_click()
            self._set_anim('click')
            pet_top_center = QPoint(
                self.frameGeometry().left() + self.width() // 2,
                self.frameGeometry().top(),
            )
            self.chat_bubble.show_message(
                random.choice(self._CLICK_PHRASES),
                pet_top_center,
            )
            event.accept()

        elif event.button() == Qt.MouseButton.RightButton:
            self.tray.show(event.globalPos().x(), event.globalPos().y())
            event.accept()

    def mouseMoveEvent(self, event):
        if event.buttons() == Qt.MouseButton.LeftButton and self._drag_start_pos:
            self.move(event.globalPos() - self._drag_start_pos)
            if self._anim != 'drag':
                self._pet_state.on_drag()
                self._set_anim('drag')
            if self.chat_bubble.isVisible():
                pet_top_center = QPoint(
                    self.frameGeometry().left() + self.width() // 2,
                    self.frameGeometry().top(),
                )
                self.chat_bubble.move(
                    pet_top_center.x() - self.chat_bubble.width() // 2,
                    pet_top_center.y() - self.chat_bubble.height() - 10,
                )
            event.accept()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            was_dragging = self._anim == 'drag'
            drag_duration = (time.monotonic() - self._drag_start_time) if self._drag_start_time else 1.0
            self._drag_start_pos = None
            self._drag_start_time = None

            if was_dragging:
                rough = drag_duration < 0.5
                self._pet_state.on_drag_release(rough=rough)
                self._set_anim('idle')

            event.accept()

    # ── 附属 UI 逻辑转换 ──────────────────────────────────────────────────
    def _update_badge(self):
        """番茄钟状态标签 (替代原来的 _update_badge 和 _badge_job)"""
        if self._pom.phase == 'idle':
            if hasattr(self, '_badge_lbl'):
                self._badge_lbl.hide()
            self.badge_timer.stop()
            return

        # 初始化 Badge Label
        if not hasattr(self, '_badge_lbl'):
            self._badge_lbl = QLabel(self)
            self._badge_lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
            self._badge_lbl.show()

        phase_color = '#1565C0' if self._pom.phase == 'break' else '#D32F2F'
        self._badge_lbl.setText(self._pom.remaining_str)
        self._badge_lbl.setStyleSheet(f"""
            background-color: {phase_color}; 
            color: white; 
            border-radius: 5px; 
            padding: 2px 6px; 
            font-weight: bold;
            font-family: 'Segoe UI';
        """)
        
        self._badge_lbl.adjustSize()
        # 放在宠物右上角
        self._badge_lbl.move(self.width() - self._badge_lbl.width() - 5, 5)
        self._badge_lbl.show()
        
        # 确保持续循环
        if not self.badge_timer.isActive():
            self.badge_timer.start(1000)

    def _on_about(self) -> None:
        """弹出模态关于对话框，展示宠物当前状态数值"""
        s = self._pet_state
        status = (f'Mood: {s.mood.value}\n'
                  f'Energy: {s.energy:.0f}\n'
                  f'Affection: {s.affection:.0f}')
        
        # 使用 PySide6 原生的消息框，替代 tk.Toplevel
        QMessageBox.about(
            self, 
            'About Mutsumi',
            '🌸  Mutsumi Desktop Pet  🌸\n\n'
            'Wakaba Mutsumi always by your side~\n\n'
            f'{status}'
        )

    def _on_settings_change(self, new_settings: Settings):
        self._settings = new_settings
        
        # 1. 应用健康提醒设置
        self._activity.set_stretch_enabled(self._settings.stretch_reminders_on)
        
        # 2. 应用番茄钟时长更改
        self._pom.focus_mins = self._settings.pom_focus_mins
        self._pom.break_mins = self._settings.pom_break_mins
        
        # 3. 持久化存储：调用之前 Persistence 模块的 save 方法
        # 将最新的设置对象转为字典并保存到 settings.json
        self._persistence.save(
            'settings.json', 
            self._settings.to_dict, 
            debounce_ms=2000  # 2秒防抖，避免频繁写入磁盘
        )
    def _on_reset(self):
        self._pet_state = PetState()
        self.state_timer.timeout.disconnect()
        self.state_timer.timeout.connect(self._pet_state.tick)
        self._persistence.save_now('state.json', {
            'energy': self._pet_state.energy,
            'affection': self._pet_state.affection,
        })