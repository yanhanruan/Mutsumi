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
        # 主动画循环定时器 — 固定 8 ms 轮询，由 _animate_step 内部根据
        # 锁步时钟（_last_frame_t += interval）决定是否推进帧，避免抖动。
        self._last_frame_t = time.monotonic()
        self.anim_timer = QTimer(self)
        self.anim_timer.timeout.connect(self._animate_step)
        self.anim_timer.start(8)  # fixed poll; actual fps driven by elapsed time

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
        self._drag_start_global: Optional[QPoint] = None  # for distinguishing click vs drag
        self._did_drag = False  # set to True once movement exceeds threshold

        # 番茄钟事件订阅 — 开始时启动 badge 刷新
        events.subscribe('pomodoro_focus_start', lambda **_: self.badge_timer.start(1000))
        events.subscribe('pomodoro_break_start', lambda **_: self.badge_timer.start(1000))

        # ── 音频检测与动画待机队列 ──
        self._pending_anim: Optional[str] = None

        # Stop is debounced by 3 s to ignore brief gaps (song transitions, buffering).
        # Start fires immediately.
        self._audio_stop_timer = QTimer(self)
        self._audio_stop_timer.setSingleShot(True)
        self._audio_stop_timer.timeout.connect(self._end_music_sequence)

        from audio_detector import AudioDetector
        self._audio = AudioDetector(poll_ms=500, parent=self)
        self._audio.audio_started.connect(self._on_audio_started)
        self._audio.audio_stopped.connect(self._on_audio_stopped)

    # ── 动画加载 ──────────────────────────────────────────────────────────
    _DISPLAY_HEIGHT = 200  # px; change to scale the pet up or down

    @staticmethod
    def _assets_dir() -> str:
        src_dir = os.path.dirname(os.path.abspath(__file__))
        return os.path.join(os.path.dirname(src_dir), 'assets')

    @staticmethod
    def _load_dir_anim(name: str, fps: float, loop: bool) -> Animation:
        frames_dir = os.path.join(DesktopPet._assets_dir(), name)
        paths = sorted(
            f for f in (os.path.join(frames_dir, n) for n in os.listdir(frames_dir))
            if f.lower().endswith('.webp')
        )
        h = DesktopPet._DISPLAY_HEIGHT
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
            'idle':       self._load_dir_anim('idle',          fps=24.0, loop=True),
            'click':      self._load_dir_anim('click_matched', fps=24.0, loop=False),
            'headphones_on':  self._load_dir_anim('headphones_on',  fps=24.0, loop=False),
            'headphones_off': self._load_dir_anim('headphones_off', fps=24.0, loop=False),
            'music':          self._load_dir_anim('music',          fps=24.0, loop=True),
            'music2':         self._load_dir_anim('music2',         fps=24.0, loop=True),
            'drag':       self._pil_anim(generate_drag_frames(char),  fps=10, loop=True),  # unused — kept for future use
            'fly':        self._pil_anim(generate_fly_frames(char),   fps=16, loop=True),
            'dizzy':      self._pil_anim(generate_dizzy_frames(char), fps=10, loop=False),
            'squash':     self._pil_anim(squash_pil,                  fps=12, loop=False),
            'rebound':    self._pil_anim(rebound_pil,                 fps=12, loop=False),
        }

    # ── 动画渲染核心 ──────────────────────────────────────────────────────
    def _animate_step(self):
        now = time.monotonic()
        interval = self._animations[self._anim].interval_ms / 1000.0
        elapsed = now - self._last_frame_t
        if elapsed < interval:
            return  # not yet time for the next frame
        # Clamp: if we're more than 2 frames behind (startup lag or stall),
        # reset instead of catching up — prevents rapid-fire frame bursts.
        if elapsed > interval * 2:
            self._last_frame_t = now
        else:
            self._last_frame_t += interval  # locked-step

        try:
            pixmap: QPixmap = next(self._current_frame_generator)
            self.pet_label.setPixmap(pixmap)
            # Only resize when dimensions actually change — avoids hammering
            # the DWM compositor on every frame of a same-size animation.
            w, h = pixmap.width(), pixmap.height()
            if self.width() != w or self.height() != h:
                self.pet_label.resize(w, h)
                self.resize(w, h)
        except StopIteration:
            print(f'[pet] StopIteration: anim={self._anim!r}, pending={self._pending_anim!r}')
            if self._pending_anim:
                nxt, self._pending_anim = self._pending_anim, None
                print(f'[pet] → switching to pending {nxt!r}')
                self._set_anim(nxt)
            elif self._anim == 'headphones_on':
                self._set_anim('music')
            elif self._anim == 'music':
                self._set_anim('music2')
            elif self._anim == 'music2':
                self._set_anim('music')
            elif self._anim == 'headphones_off':
                self._set_anim('idle')
            elif self._animations[self._anim].loop:
                self._set_anim(self._anim)
            else:
                self._set_anim('idle')

    def _set_anim(self, anim_name: str):
        if anim_name not in self._animations:
            return
        
        self._anim = anim_name
        anim_obj = self._animations[anim_name]
        
        # 重新生成迭代器，确保从第一帧开始播放
        self._current_frame_generator = iter(anim_obj.frames)
        # Reset the locked-step clock so the new animation's first frame
        # fires on the next poll instead of waiting a leftover interval.
        self._last_frame_t = time.monotonic()
            
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
            self._drag_start_global = event.globalPos()
            self._drag_start_time = time.monotonic()
            self._did_drag = False
            event.accept()

        elif event.button() == Qt.MouseButton.RightButton:
            self.tray.show(event.globalPos().x(), event.globalPos().y())
            event.accept()

    _DRAG_THRESHOLD = 5  # pixels of movement before treating press as a drag

    def mouseMoveEvent(self, event):
        if event.buttons() == Qt.MouseButton.LeftButton and self._drag_start_pos:
            if not self._did_drag:
                delta = event.globalPos() - self._drag_start_global
                if delta.manhattanLength() >= self._DRAG_THRESHOLD:
                    self._did_drag = True
            self.move(event.globalPos() - self._drag_start_pos)
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
            was_drag = self._did_drag
            self._drag_start_pos = None
            self._drag_start_global = None
            self._drag_start_time = None
            self._did_drag = False

            if not was_drag:
                # True click — fire click logic only now
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

    # ── 音频响应 ──────────────────────────────────────────────────────────

    def _on_audio_started(self):
        self._audio_stop_timer.stop()   # cancel any pending stop debounce
        self._begin_music_sequence()

    def _on_audio_stopped(self):
        self._audio_stop_timer.start(3000)  # wait 3 s before acting

    def _begin_music_sequence(self):
        print(f'[pet] _begin_music_sequence: anim={self._anim!r}, pending={self._pending_anim!r}')
        if self._anim not in ('headphones_on', 'headphones_off', 'music', 'music2'):
            self._pending_anim = 'headphones_on'
            print('[pet] pending → headphones_on')
            if self._anim == 'idle':
                # Fast-forward idle so StopIteration fires on the next timer tick.
                self._current_frame_generator = iter([])

    def _end_music_sequence(self):
        print(f'[pet] _end_music_sequence: anim={self._anim!r}, pending={self._pending_anim!r}')
        if self._anim in ('headphones_on', 'music', 'music2'):
            self._pending_anim = 'headphones_off'
            print('[pet] pending → headphones_off')
        elif self._anim == 'headphones_off':
            pass  # already on the way out
        elif self._pending_anim == 'headphones_on':
            self._pending_anim = None
            print('[pet] cancelled pending headphones_on')  # cancel queued music start