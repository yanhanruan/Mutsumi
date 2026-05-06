"""
持久化模块
框架：PySide6
将宠物状态与设置以 JSON 格式保存，支持防抖写入。
"""
from __future__ import annotations
import json
from pathlib import Path
from typing import Callable, Dict, Optional
from PySide6.QtCore import QObject, QTimer

class Persistence(QObject):
    def __init__(self, parent: QObject = None, data_dir: Optional[Path] = None) -> None:
        super().__init__(parent)
        self._dir = data_dir or (Path.home() / '.mutsumi')
        self._dir.mkdir(parents=True, exist_ok=True)
        
        # 保存防抖的 QTimer 对象字典
        self._timers: Dict[str, QTimer] = {}
        self._pending: Dict[str, Callable[[], dict]] = {}

    def load(self, filename: str, default: dict) -> dict:
        path = self._dir / filename
        try:
            with open(path, encoding='utf-8') as fh:
                data = json.load(fh)
            print(f'[Persistence] Loaded {filename}')
            return data
        except Exception as exc:
            print(f'[Persistence] Load failed or not found {filename}: using defaults')
        return dict(default)

    def save(self, filename: str, get_data: Callable[[], dict], debounce_ms: int = 5000) -> None:
        self._pending[filename] = get_data
        
        if filename not in self._timers:
            timer = QTimer(self)
            timer.setSingleShot(True)
            timer.timeout.connect(lambda f=filename: self._flush(f))
            self._timers[filename] = timer
            
        self._timers[filename].start(debounce_ms)

    def save_now(self, filename: str, data: dict) -> None:
        if filename in self._timers:
            self._timers[filename].stop()
        self._pending.pop(filename, None)
        self._write(filename, data)

    def _flush(self, filename: str) -> None:
        get_data = self._pending.pop(filename, None)
        if get_data is not None:
            self._write(filename, get_data())

    def _write(self, filename: str, data: dict) -> None:
        path = self._dir / filename
        tmp = path.with_suffix('.tmp')
        try:
            with open(tmp, 'w', encoding='utf-8') as fh:
                json.dump(data, fh, ensure_ascii=False, indent=2)
            tmp.replace(path)
            print(f'[Persistence] Saved {filename}')
        except Exception as exc:
            print(f'[Persistence] Save failed {filename}: {exc}')