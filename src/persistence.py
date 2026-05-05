"""
持久化模块
将宠物状态与设置以 JSON 格式保存到 ~/.mutsumi/，支持防抖写入。
"""
from __future__ import annotations
import json
import os
import tkinter as tk
from pathlib import Path
from typing import Callable, Dict, Optional


class Persistence:
    """
    轻量级 JSON 持久化帮助类。
    save() 采用防抖策略：同一文件在 debounce_ms 内多次调用只写入一次。
    save_now() 立即同步写入（用于退出前刷新）。
    """

    def __init__(self, root: tk.Tk, data_dir: Optional[Path] = None) -> None:
        self._root = root
        self._dir  = data_dir or (Path.home() / '.mutsumi')
        self._dir.mkdir(parents=True, exist_ok=True)
        # filename → after() job id
        self._jobs: Dict[str, str] = {}
        # filename → pending get_data callable
        self._pending: Dict[str, Callable[[], dict]] = {}

    # ── 公开接口 ──────────────────────────────────────────────────────────

    def load(self, filename: str, default: dict) -> dict:
        """从磁盘读取 JSON；文件缺失或损坏时返回 default 的副本。"""
        path = self._dir / filename
        try:
            with open(path, encoding='utf-8') as fh:
                data = json.load(fh)
            print(f'[Persistence] Loaded {filename}')
            return data
        except FileNotFoundError:
            print(f'[Persistence] {filename} not found — using defaults')
        except Exception as exc:
            print(f'[Persistence] Failed to load {filename}: {exc} — using defaults')
        return dict(default)

    def save(
        self,
        filename: str,
        get_data: Callable[[], dict],
        debounce_ms: int = 5000,
    ) -> None:
        """
        防抖保存：debounce_ms 内多次调用只触发最后一次写入。
        get_data 在真正写入时才被调用，确保拿到最新数据。
        """
        # 取消上一次未触发的写入
        if filename in self._jobs:
            self._root.after_cancel(self._jobs[filename])
        self._pending[filename] = get_data
        self._jobs[filename]    = self._root.after(
            debounce_ms,
            lambda: self._flush(filename),
        )

    def save_now(self, filename: str, data: dict) -> None:
        """立即写入（退出前调用）。若有待处理的防抖任务，先取消它。"""
        if filename in self._jobs:
            self._root.after_cancel(self._jobs.pop(filename))
        self._pending.pop(filename, None)
        self._write(filename, data)

    # ── 内部方法 ──────────────────────────────────────────────────────────

    def _flush(self, filename: str) -> None:
        self._jobs.pop(filename, None)
        get_data = self._pending.pop(filename, None)
        if get_data is not None:
            self._write(filename, get_data())

    def _write(self, filename: str, data: dict) -> None:
        path = self._dir / filename
        try:
            with open(path, 'w', encoding='utf-8') as fh:
                json.dump(data, fh, indent=2, ensure_ascii=False)
        except Exception as exc:
            print(f'[Persistence] Failed to write {filename}: {exc}')
