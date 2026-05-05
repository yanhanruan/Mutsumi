"""
宠物状态模块
定义 Mood 枚举和 PetState 数据类
tick() 每 5 秒由 pet.py 通过 root.after() 驱动，在主线程中执行（无线程安全问题）
"""
from __future__ import annotations
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum

import events


class Mood(Enum):
    """宠物心情枚举，由能量和亲密度阈值共同决定"""
    HAPPY   = "happy"
    NEUTRAL = "neutral"
    SLEEPY  = "sleepy"
    BORED   = "bored"
    EXCITED = "excited"


# ── 数值衰减 / 恢复常量（每 5 秒一次 tick）──
_ENERGY_DECAY    = 0.5    # 每 tick 能量自然衰减
_AFFECTION_DECAY = 0.15   # 每 tick 亲密度自然衰减
_IDLE_EXTRA_DECAY = 0.30  # 长时间无互动时亲密度额外衰减

# 超过此秒数未互动，判定为"冷落"
_IDLE_BORED_THRESHOLD_SECS = 120


@dataclass
class PetState:
    """
    记录宠物当前状态的数据类。
    所有字段均可被外部直接读取（用于 UI 显示或调试）。
    状态变更通过 events 总线广播，不直接持有 UI 引用。
    """

    energy:              float    = 100.0
    affection:           float    =  60.0
    mood:                Mood     = Mood.NEUTRAL
    last_interaction_at: datetime = field(default_factory=datetime.now)

    # ── 内部工具 ──────────────────────────────────────────────────────────

    @staticmethod
    def _clamp(value: float, lo: float = 0.0, hi: float = 100.0) -> float:
        """将数值限制在 [lo, hi] 区间内"""
        return max(lo, min(hi, value))

    def _recalc_mood(self) -> None:
        """
        根据当前能量和亲密度重新计算心情。
        优先级（高→低）：SLEEPY > EXCITED > BORED > HAPPY > NEUTRAL
        若心情发生变化，向事件总线发布 'mood_changed'。
        """
        old = self.mood

        if self.energy < 20:
            new = Mood.SLEEPY
        elif self.affection > 75 and self.energy > 60:
            new = Mood.EXCITED
        elif self.affection < 25:
            new = Mood.BORED
        elif self.energy >= 50 and self.affection >= 50:
            new = Mood.HAPPY
        else:
            new = Mood.NEUTRAL

        if new != old:
            self.mood = new
            events.publish('mood_changed', old_mood=old, new_mood=new)

    # ── 公开接口 ──────────────────────────────────────────────────────────

    def tick(self) -> None:
        """
        每 5 秒调用一次：衰减能量与亲密度，重新评估心情。
        长时间未互动时亲密度额外衰减，加速进入 BORED 状态。
        """
        self.energy    = self._clamp(self.energy    - _ENERGY_DECAY)
        self.affection = self._clamp(self.affection - _AFFECTION_DECAY)

        # 超过阈值未互动 → 亲密度额外衰减
        idle_secs = (datetime.now() - self.last_interaction_at).total_seconds()
        if idle_secs > _IDLE_BORED_THRESHOLD_SECS:
            self.affection = self._clamp(self.affection - _IDLE_EXTRA_DECAY)

        self._recalc_mood()
        events.publish('tick', state=self)

    def on_click(self) -> None:
        """用户点击宠物：提升亲密度和少量能量，更新互动时间"""
        self.affection           = self._clamp(self.affection + 5.0)
        self.energy              = self._clamp(self.energy    + 2.0)
        self.last_interaction_at = datetime.now()
        self._recalc_mood()
        events.publish('interaction', kind='click', state=self)

    def on_drag(self) -> None:
        """用户拖拽宠物：提升少量亲密度，更新互动时间"""
        self.affection           = self._clamp(self.affection + 2.0)
        self.last_interaction_at = datetime.now()
        self._recalc_mood()
        events.publish('interaction', kind='drag', state=self)

    def on_drag_release(self, rough: bool) -> None:
        """
        拖拽结束时调用，根据拖拽时长决定亲密度变化。
        rough=True（持续 < 0.5 秒）：粗暴对待，亲密度 -3
        rough=False（持续 ≥ 0.5 秒）：温柔搬运，无额外变化
        注：on_drag() 已在拖拽开始时 +2，rough 时合计净变化 = -1。
        """
        if rough:
            self.affection = self._clamp(self.affection - 3.0)
            self._recalc_mood()
        events.publish('interaction',
                       kind='drag_rough' if rough else 'drag_gentle',
                       state=self)

    def on_pet(self) -> None:
        """
        快速连点 3 次以上触发的"抚摸"响应：在普通点击之外额外奖励亲密度。
        pet.py 在检测到连点时调用，on_click() 仍照常执行。
        """
        self.affection           = self._clamp(self.affection + 5.0)
        self.last_interaction_at = datetime.now()
        self._recalc_mood()
        events.publish('interaction', kind='pet', state=self)

    # ── 只读属性（供 UI 展示用）──────────────────────────────────────────

    @property
    def idle_seconds(self) -> float:
        """距上次互动已过去的秒数"""
        return (datetime.now() - self.last_interaction_at).total_seconds()
