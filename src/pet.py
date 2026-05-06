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

# 🔴 移除所有的 import tkinter，全面替换为 PySide6
from PySide6.QtWidgets import QWidget, QLabel, QMessageBox, QApplication
from PySide6.QtCore import Qt, QTimer, QPoint
from PySide6.QtGui import QPixmap, QCursor, QImage

# 🔴 引入 ImageQt 用于将 PIL 动画帧转为 Qt 格式
from PIL import Image
from PIL.ImageQt import ImageQt

import events
from animator     import (Animation,
                           generate_idle_frames, generate_click_frames,
                           generate_drag_frames, generate_fly_frames,
                           generate_dizzy_frames, generate_squash_frames,
                           load_sprite_sheet, build_animation,
                           CHROMA_HEX, WINDOW_SIZE)
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
        
        # TODO: 这里调用你原本的动画加载逻辑
        self._animations = self._load_all_animations()
        self._current_frame_generator = self._get_anim_generator(self._anim)

        # ── 4. 初始化附属 UI 组件 ──
        self.chat_bubble = ChatBubble()
        self.settings_window = SettingsWindow(self._settings, self._on_settings_change, self._on_reset)
        self.z_particles = ZParticles(self)
        
        self.tray = ContextMenu(
            parent=self,
            on_exit=QApplication.instance().quit,
            on_about=self._on_about,
            on_pom_start=self._pom.start,
            on_pom_pause=self._pom.pause,
            on_pom_stop=self._pom.stop,
            # 其他你原本绑定的回调...
        )

        # ── 5. 替换原本的 root.after 定时器 ──
        # 主动画循环定时器 (替代 root.after(100, update))
        self.anim_timer = QTimer(self)
        self.anim_timer.timeout.connect(self._animate_step)
        self.anim_timer.start(110) # 默认 110ms 一帧

        # 番茄钟 Badge 刷新定时器
        self.badge_timer = QTimer(self)
        self.badge_timer.timeout.connect(self._update_badge)

        # 物理拖拽变量
        self._drag_start_pos = None

    # ── 动画渲染核心 ──────────────────────────────────────────────────────
    def _animate_step(self):
        """替代原来的逐帧渲染逻辑"""
        try:
            # 提取下一帧 (类型为 PIL.Image.Image)
            pil_frame = next(self._current_frame_generator)
            
            if pil_frame:
                # 【核心转换魔法】：PIL Image -> QImage -> QPixmap
                # 使用 ImageQt 将 PIL 原生转换为 PySide6 可识别的高清透明图像
                qimage = ImageQt(pil_frame)
                pixmap = QPixmap.fromImage(qimage)
                
                self.pet_label.setPixmap(pixmap)
                
                # 动态调整窗口大小以适应动画帧
                self.pet_label.resize(pixmap.width(), pixmap.height())
                self.resize(pixmap.width(), pixmap.height())
                
        except StopIteration:
            # 当前动画序列播放完毕，切回待机状态
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
    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_start_pos = event.globalPos() - self.frameGeometry().topLeft()
            self._set_anim('click')
            event.accept()
            
        elif event.button() == Qt.MouseButton.RightButton:
            # 右键弹出菜单
            self.tray.show(event.globalPos().x(), event.globalPos().y())
            event.accept()

    def mouseMoveEvent(self, event):
        if event.buttons() == Qt.MouseButton.LeftButton and self._drag_start_pos:
            self.move(event.globalPos() - self._drag_start_pos)
            if self._anim != 'drag':
                self._set_anim('drag')
            event.accept()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_start_pos = None
            
            # 物理抛出逻辑简述：
            # 如果你之前在 mouseMoveEvent 里记录了位移增量（velocity），
            # 可以在这里判断速度是否超过阈值。
            # 暂时先回归最稳妥的逻辑：
            
            if self._anim == 'drag':
                # 如果你想让它有“落地”动作，可以先切到 squash 再切回 idle
                # 这里我们直接恢复待机
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
        # 更新显示...