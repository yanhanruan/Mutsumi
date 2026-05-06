# ui_theme.py
class Theme:
    # 主色调：高级鼠尾草绿
    PRIMARY = "#779977"
    PRIMARY_HOVER = "#88AA88"
    
    # 玻璃拟态基础样式 (用于 QFrame)
    GLASS_STYLE = f"""
        QFrame#GlassPanel {{
            background-color: rgba(255, 255, 255, 0.6);
            border: 1px solid rgba(255, 255, 255, 0.8);
            border-radius: 15px;
        }}
        QLabel {{
            color: #333333;
            font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
            font-size: 13px;
        }}
        QCheckBox {{
            spacing: 8px;
            font-size: 13px;
            color: #333333;
        }}
        QCheckBox::indicator {{
            width: 18px;
            height: 18px;
            border-radius: 4px;
            border: 1px solid #CCCCCC;
            background-color: rgba(255, 255, 255, 0.7);
        }}
        QCheckBox::indicator:checked {{
            background-color: {PRIMARY};
            border: 1px solid {PRIMARY};
        }}
        QSlider::groove:horizontal {{
            border: none;
            height: 6px;
            background: rgba(0, 0, 0, 0.1);
            border-radius: 3px;
        }}
        QSlider::sub-page:horizontal {{
            background: {PRIMARY};
            border-radius: 3px;
        }}
        QSlider::handle:horizontal {{
            background: white;
            border: 1px solid {PRIMARY};
            width: 14px;
            margin: -4px 0;
            border-radius: 7px;
        }}
        QPushButton {{
            background-color: transparent;
            color: {PRIMARY};
            border: 1px solid {PRIMARY};
            border-radius: 5px;
            padding: 5px 15px;
            font-weight: bold;
        }}
        QPushButton:hover {{
            background-color: {PRIMARY};
            color: white;
        }}
    """

    # 气泡专用样式
    BUBBLE_STYLE = """
        QFrame#BubblePanel {
            background-color: rgba(255, 255, 255, 0.85);
            border: 1px solid rgba(255, 255, 255, 0.9);
            border-radius: 12px;
        }
        QLabel {
            color: #444444;
            font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
            font-size: 14px;
        }
    """