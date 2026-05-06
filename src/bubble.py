# bubble.py
from PySide6.QtWidgets import QWidget, QVBoxLayout, QLabel, QFrame
from PySide6.QtCore import Qt, QTimer
from ui_theme import Theme

class ChatBubble(QWidget):
    def __init__(self):
        super().__init__()
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | 
                            Qt.WindowType.WindowStaysOnTopHint | 
                            Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        
        layout = QVBoxLayout(self)
        self.glass_panel = QFrame(self)
        self.glass_panel.setObjectName("BubblePanel")
        self.glass_panel.setStyleSheet(Theme.BUBBLE_STYLE)
        
        panel_layout = QVBoxLayout(self.glass_panel)
        self.text_label = QLabel("I'm not a toy, you know!")
        self.text_label.setWordWrap(True)
        self.text_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        panel_layout.addWidget(self.text_label)
        
        layout.addWidget(self.glass_panel)
        self.hide()
        
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.hide)

    def show_message(self, text, global_pos, duration=3000):
        self.text_label.setText(text)
        self.adjustSize()
        # 居中显示在宠物上方
        self.move(global_pos.x() - self.width() // 2, global_pos.y() - self.height() - 10)
        self.show()
        self.timer.start(duration)