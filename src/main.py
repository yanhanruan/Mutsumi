"""
桌面宠物启动入口
运行方式：python src/main.py
"""
import sys
import os

# 确保 src/ 目录在模块搜索路径中（兼容直接运行和 PyInstaller 打包）
_SRC_DIR = os.path.dirname(os.path.abspath(__file__))
if _SRC_DIR not in sys.path:
    sys.path.insert(0, _SRC_DIR)

from PySide6.QtWidgets import QApplication
from pet import DesktopPet

# Raise Windows multimedia timer resolution to 1 ms so QTimer fires at the
# requested interval instead of the default ~15 ms system clock granularity.
try:
    import ctypes
    ctypes.windll.winmm.timeBeginPeriod(1)
    _TIMER_PERIOD_SET = True
except Exception:
    _TIMER_PERIOD_SET = False


def main():
    # 【核心修复】：全面弃用 tkinter，整个程序生命周期只允许一个 QApplication
    app = QApplication(sys.argv)
    
    # 开启 Fusion 风格可以确保我们自定义的 QSS（包括透明度和圆角）在所有系统上正常渲染
    app.setStyle("Fusion") 

    # 初始化宠物。注意：你的 pet.py 也需要是基于 PySide6 QWidget 开发的。
    # PySide6 的无边框主窗口通常不需要显式传入 root，所以这里不传参数或传 None。
    _pet = DesktopPet()
    
    # 如果 DesktopPet 的 __init__ 里没有自启动显示，可以通过 _pet.show() 调用
    if hasattr(_pet, 'show'):
        _pet.show()

    # 进入 PySide6 事件循环
    result = app.exec()

    if _TIMER_PERIOD_SET:
        try:
            ctypes.windll.winmm.timeEndPeriod(1)
        except Exception:
            pass

    sys.exit(result)


if __name__ == '__main__':
    main()