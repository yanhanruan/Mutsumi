"""
睡眠 Z 粒子特效
框架：PySide6

当宠物睡眠时在宠物头顶生成上升并淡出的 Z 字符。
"""
from __future__ import annotations
import random
import time
from PySide6.QtWidgets import QWidget
from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import QPainter, QColor, QFont

class ZParticles(QWidget):
    _LIFE_MS  = 2000
    _SPAWN_MS = 1500
    _RISE_PX  = 30
    _CANVAS_W = 80
    _CANVAS_H = 50

    def __init__(self, parent: QWidget) -> None:
        super().__init__(parent)
        self.setFixedSize(self._CANVAS_W, self._CANVAS_H)
        # 鼠标穿透
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        
        self._font = QFont('Segoe UI', 9, QFont.Weight.Bold)
        self._texts = ('z', 'Z', 'Zzz')
        self._particles = []
        
        self._spawn_timer = QTimer(self)
        self._spawn_timer.timeout.connect(self._spawn_z)
        
        self._anim_timer = QTimer(self)
        self._anim_timer.timeout.connect(self.update) # 触发重绘

        self.hide()

    def start(self) -> None:
        self._particles.clear()
        self._spawn_timer.start(self._SPAWN_MS)
        self._anim_timer.start(50) # 约 20fps
        self.show()

    def stop(self) -> None:
        self._spawn_timer.stop()
        self._anim_timer.stop()
        self._particles.clear()
        self.hide()

    def _spawn_z(self) -> None:
        text = self._texts[len(self._particles) % len(self._texts)]
        x = random.randint(12, self._CANVAS_W - 12)
        y_base = self._CANVAS_H - 6
        self._particles.append({'text': text, 'start_t': time.monotonic(), 'x': x, 'y_base': y_base})

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setFont(self._font)
        now = time.monotonic()
        alive = []
        
        for p in self._particles:
            t = (now - p['start_t']) / (self._LIFE_MS / 1000.0)
            if t >= 1.0:
                continue
            y = p['y_base'] - int(t * self._RISE_PX)
            
            # 淡出效果 (透明度从 255 降到 0)
            alpha = max(0, int(255 * (1.0 - t)))
            painter.setPen(QColor(255, 255, 255, alpha))
            painter.drawText(p['x'], y, p['text'])
            alive.append(p)
            
        self._particles = alive