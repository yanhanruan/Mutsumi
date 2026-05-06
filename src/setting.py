# settings.py
from PySide6.QtWidgets import (QWidget, QVBoxLayout, QLabel, QFrame, 
                               QCheckBox, QSlider, QPushButton, QHBoxLayout, QGroupBox)
from PySide6.QtCore import Qt
from ui_theme import Theme

class SettingsWindow(QWidget):
    def __init__(self):
        super().__init__()
        # 核心：无边框 + 背景透明
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.resize(350, 450)
        
        self.init_ui()
        
        # 拖拽相关变量
        self.drag_pos = None

    def init_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 10, 10, 10)

        # 创建玻璃面板容器
        self.glass_panel = QFrame(self)
        self.glass_panel.setObjectName("GlassPanel")
        self.glass_panel.setStyleSheet(Theme.GLASS_STYLE)
        panel_layout = QVBoxLayout(self.glass_panel)
        panel_layout.setSpacing(15)

        # 标题
        title = QLabel("✦ Mutsumi Settings")
        title.setStyleSheet(f"font-size: 16px; font-weight: bold; color: {Theme.PRIMARY};")
        panel_layout.addWidget(title)

        # 区域 1: Personality
        self.setup_section(panel_layout, "Personality", ["Idle chatter"], has_slider=True, slider_label="Frequency (min):")
        
        # 区域 2: Pomodoro
        self.setup_section(panel_layout, "Pomodoro", [], has_slider=True, slider_label="Focus length (min):")
        
        # 区域 3: Wellness
        self.setup_section(panel_layout, "Wellness", ["Stretch reminders"], has_slider=True, slider_label="Master volume:")

        # 底部按钮
        btn_layout = QHBoxLayout()
        reset_btn = QPushButton("Reset")
        reset_btn.setStyleSheet("color: #999999; border-color: #DDDDDD;")
        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.hide)
        btn_layout.addWidget(reset_btn)
        btn_layout.addStretch()
        btn_layout.addWidget(close_btn)
        panel_layout.addLayout(btn_layout)

        layout.addWidget(self.glass_panel)

    def setup_section(self, parent_layout, title, checkboxes, has_slider=False, slider_label=""):
        group = QFrame()
        layout = QVBoxLayout(group)
        layout.setContentsMargins(0, 5, 0, 10)
        
        title_label = QLabel(title)
        title_label.setStyleSheet("font-weight: bold; color: #555555;")
        layout.addWidget(title_label)
        
        for cb_text in checkboxes:
            layout.addWidget(QCheckBox(cb_text))
            
        if has_slider:
            slider_layout = QHBoxLayout()
            slider_layout.addWidget(QLabel(slider_label))
            slider = QSlider(Qt.Orientation.Horizontal)
            slider.setValue(50)
            slider_layout.addWidget(slider)
            layout.addLayout(slider_layout)
            
        parent_layout.addWidget(group)

    # 实现窗口拖拽
    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.drag_pos = event.globalPos() - self.frameGeometry().topLeft()
            event.accept()

    def mouseMoveEvent(self, event):
        if event.buttons() == Qt.MouseButton.LeftButton and self.drag_pos:
            self.move(event.globalPos() - self.drag_pos)
            event.accept()