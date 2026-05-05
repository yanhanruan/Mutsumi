"""
事件总线模块
轻量级的发布/订阅机制，用于各模块之间的解耦通信
无外部依赖，模块级单例（应用内全局共享）
"""
from __future__ import annotations
from collections import defaultdict
from typing import Callable, Any, Dict, List

# 订阅者注册表：事件名 → 处理函数列表
_subscribers: Dict[str, List[Callable[..., Any]]] = defaultdict(list)


def subscribe(event: str, handler: Callable[..., Any]) -> None:
    """注册事件处理函数。同一 handler 可重复注册（会被多次调用）。"""
    _subscribers[event].append(handler)


def unsubscribe(event: str, handler: Callable[..., Any]) -> None:
    """注销已注册的处理函数（若未注册则静默忽略）。"""
    try:
        _subscribers[event].remove(handler)
    except ValueError:
        pass


def publish(event: str, **kwargs: Any) -> None:
    """
    向所有订阅者广播事件，附带关键字参数。
    使用列表副本迭代，允许处理函数内部安全地调用 unsubscribe。
    """
    for handler in list(_subscribers[event]):
        handler(**kwargs)


def clear(event: str = '') -> None:
    """
    清空订阅者（测试用）。
    若 event 为空字符串则清空所有事件，否则只清空指定事件。
    """
    if event:
        _subscribers[event].clear()
    else:
        _subscribers.clear()
