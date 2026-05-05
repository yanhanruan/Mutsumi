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

import tkinter as tk
from pet import DesktopPet


def main():
    root = tk.Tk()
    root.title('Mutsumi Pet')   # 仅在 Alt+Tab 列表中可见，窗口本身无标题栏

    # 初始化宠物（窗口配置、图像加载、动画启动均在 DesktopPet.__init__ 中完成）
    _pet = DesktopPet(root)

    root.mainloop()


if __name__ == '__main__':
    main()
