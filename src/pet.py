"""
桌面宠物主窗口模块
负责：窗口创建与透明化、动画状态机、鼠标交互、PetState 驱动、气泡与菜单调用

命名约定
  self._anim      → 当前动画状态字符串 ('idle' | 'click' | 'drag')
  self._pet_state → PetState 实例（能量 / 亲密度 / 心情）
"""
from __future__ import annotations
import math
import os
import sys
import random
import time
import tkinter as tk
from datetime import datetime
from typing import Dict, List, Optional, Tuple

from PIL import Image, ImageTk

import events
from animator     import (Animation,
                           generate_idle_frames, generate_click_frames,
                           generate_drag_frames, generate_fly_frames,
                           generate_dizzy_frames, generate_squash_frames,
                           load_sprite_sheet, build_animation,
                           CHROMA_HEX, WINDOW_SIZE)
from activity     import ActivityMonitor
from bubble       import SpeechBubble
from persistence  import Persistence
from pomodoro     import PomodoroTimer
from settings     import Settings, SettingsWindow
from state        import PetState, Mood
from tray         import ContextMenu
from zparticles   import ZParticles


# ── 各内置动画的默认 fps ──
# Animation 对象以 fps 存储速率；此表仅作文档参考，不再直接驱动 _animate。
# 对应关系：interval_ms = 1000 / fps
#   idle       110 ms → ~9.1 fps
#   click       75 ms → ~13.3 fps
#   slow_click 160 ms → ~6.25 fps  (与 click 同帧，嗜睡时减速)
#   drag       130 ms → ~7.7 fps
#   fly         16 ms → ~62.5 fps  (物理循环直接驱动，不走 _animate)
#   dizzy      100 ms → 10 fps

# sprite sheet 中不循环播放的动画名称（播完一轮后切回 idle）
_SHEET_NON_LOOPING: frozenset = frozenset({'click', 'slow_click', 'dizzy'})

# ── 挤压弹回动画帧间隔（ms） ──
_SS_SQUASH_MS  = 16   # 5 帧覆盖 ~80ms
_SS_REBOUND_MS = 18   # 4 帧覆盖 ~70ms

# ── 抛投物理常量 ──
_THROW_THRESHOLD = 800.0   # px/s：超过此速度进入飞行状态
_FLY_GRAVITY     = 980.0   # px/s²
_FLY_DAMPING     = 0.7     # 碰壁速度衰减系数
_FLY_STOP_SPEED  =  50.0   # px/s：低于此速度判定为落地静止
_FLY_STEP_MS     =  16     # 物理更新间隔（约 60 fps）

# ── 直通台词分类：trigger 与 lines.json key 完全一致，无需额外映射 ──
_DIRECT_CATS = frozenset({
    'dragged', 'dragged_rough', 'dragged_gentle',
    'petted', 'grumpy', 'excited',
    'bored', 'idle',
    'focus_start', 'focus_done', 'break_start',
    'worried', 'welcome_back', 'stretch_reminder',
    'thrown',
})

# ── 状态 tick 间隔（毫秒） ──
TICK_MS = 5000

# ── 闲聊计时器：每隔 2–5 分钟（随机）触发一次自动台词 ──
_CHATTER_MIN_SECS = 120   # 2 分钟
_CHATTER_MAX_SECS = 300   # 5 分钟

# ── 判定"长时间冷落"的阈值（秒） ──
_BORED_IDLE_SECS = 600    # 10 分钟


class DesktopPet:
    """桌面宠物主类，管理窗口、动画、状态与所有交互逻辑"""

    def __init__(self, root: tk.Tk):
        self.root = root
        self._setup_window()

        # ── 加载角色图像 ──
        char_img = self._load_character()

        # ── 构建内置动画（Pillow 生成；sprite sheet 存在时会被覆盖） ──
        _click_anim = Animation(generate_click_frames(char_img), fps=1000/75,  loop=False)
        self._animations: Dict[str, Animation] = {
            'idle':       Animation(generate_idle_frames(char_img),  fps=1000/110, loop=True),
            'click':      _click_anim,
            'slow_click': Animation(_click_anim.frames,              fps=1000/160, loop=False),
            'drag':       Animation(generate_drag_frames(char_img),  fps=1000/130, loop=True),
            'fly':        Animation(generate_fly_frames(char_img),   fps=1000/16,  loop=True),
            'dizzy':      Animation(generate_dizzy_frames(char_img), fps=10.0,     loop=False),
        }

        # sprite sheet があれば上書き（assets/sheets/*.png）
        self._load_sheets()

        # PhotoImage キャッシュ（GC 防止のため self に保持）
        self.frames: Dict[str, List[ImageTk.PhotoImage]] = {
            name: [ImageTk.PhotoImage(f) for f in anim.frames]
            for name, anim in self._animations.items()
        }

        # ── 挤压弹回帧（独立于普通帧，由 _ss_step 直接驱动） ──
        _sq_pil, _rb_pil     = generate_squash_frames(char_img)
        self._ss_squash:  List[ImageTk.PhotoImage] = [ImageTk.PhotoImage(f) for f in _sq_pil]
        self._ss_rebound: List[ImageTk.PhotoImage] = [ImageTk.PhotoImage(f) for f in _rb_pil]

        # ── 动画状态 ──
        self._anim        = 'idle'
        self._frame_idx   = 0
        self._idle_enabled = True   # 待机浮动动画开关
        self._anim_job    = None    # after() 任务 ID

        # ── 挤压弹回动画状态 ──
        self._ss_job:   Optional[str] = None
        self._ss_phase: str           = ''   # 'squash' | 'rebound' | ''
        self._ss_idx:   int           = 0

        # ── 持久化与设置 ──
        self._persist  = Persistence(root)
        self._settings = self._load_settings()

        # ── 宠物状态（能量 / 亲密度 / 心情） ──
        self._pet_state  = self._load_state()
        self._chatter_job: Optional[str] = None   # 闲聊定时器 after() ID

        # ── 拖拽辅助变量 ──
        self._drag_mx = 0               # 按下时鼠标屏幕坐标
        self._drag_my = 0
        self._drag_wx = 0               # 按下时窗口坐标
        self._drag_wy = 0
        self._dragging = False
        self._drag_start_time: Optional[datetime] = None  # 拖拽开始时间（用于判断粗暴/温柔）
        self._motion_samples: List[Tuple[float, float, float]] = []  # (monotonic_t, x, y)

        # ── 飞行物理状态 ──
        self._flying:  bool          = False
        self._fly_vx:  float         = 0.0
        self._fly_vy:  float         = 0.0
        self._fly_x:   float         = 0.0
        self._fly_y:   float         = 0.0
        self._fly_job: Optional[str] = None

        # ── 连点检测（抚摸判定） ──
        self._click_times: List[datetime] = []   # 最近若干次点击的时间戳

        # ── 画布与角色图像 ──
        self.canvas = tk.Canvas(
            root,
            width=WINDOW_SIZE, height=WINDOW_SIZE,
            bg=CHROMA_HEX, highlightthickness=0,
        )
        self.canvas.pack()
        self._img_item = self.canvas.create_image(
            WINDOW_SIZE // 2, WINDOW_SIZE // 2, anchor=tk.CENTER,
        )

        # ── 睡眠 Z 粒子 ──
        self._z_particles = ZParticles(root)

        # ── 用户活动监控 ──
        self._activity = ActivityMonitor(root)

        # ── 番茄钟 ──
        self._pom = PomodoroTimer(root)
        # 浮动时间徽章（番茄钟运行中时显示在角色上方）
        self._badge: Optional[tk.Toplevel] = None
        self._badge_lbl: Optional[tk.Label] = None
        self._badge_job: Optional[str] = None

        # ── 子组件 ──
        self.bubble        = SpeechBubble(root, assets_dir=self._assets_dir())
        self._settings_win = SettingsWindow(
            root,
            self._settings,
            on_change=self._on_settings_change,
            on_reset_state=self._reset_pet_state,
        )
        self.menu = ContextMenu(
            root,
            on_exit=self._on_exit,
            on_toggle_animation=self._on_toggle_animation,
            on_about=self._on_about,
            on_pom_start=self._pom_start,
            on_pom_pause=self._pom_pause,
            on_pom_stop=self._pom_stop,
            on_pom_set_focus=self._pom_set_focus,
            on_stretch_toggle=self._on_stretch_toggle,
            on_settings=self._settings_win.show,
        )

        # ── 事件订阅 ──
        events.subscribe('mood_changed',            self._on_mood_changed)
        events.subscribe('pomodoro_focus_start',    self._on_pom_focus_start)
        events.subscribe('pomodoro_focus_complete', self._on_pom_focus_complete)
        events.subscribe('pomodoro_break_start',    self._on_pom_break_start)
        events.subscribe('pomodoro_break_complete', self._on_pom_break_complete)
        events.subscribe('user_idle',               self._on_user_idle)
        events.subscribe('user_returned',           self._on_user_returned)
        events.subscribe('stretch_reminder',        self._on_stretch_reminder)
        events.subscribe('tick',                    self._on_state_event)
        events.subscribe('interaction',             self._on_state_event)

        # ── 鼠标绑定 ──
        self.canvas.bind('<Button-1>',       self._on_press)
        self.canvas.bind('<B1-Motion>',      self._on_motion)
        self.canvas.bind('<ButtonRelease-1>', self._on_release)
        self.canvas.bind('<Button-3>',       self._on_right_click)

        # ── 启动动画循环、状态 tick、开场问候、设置应用、活动监控 ──
        self._start_anim()
        self._schedule_tick()
        # 延迟 600 ms 等窗口渲染完毕再弹问候气泡
        root.after(600, self._show_greeting)
        self._apply_settings(self._settings)   # 含 _reset_chatter_timer
        self._activity.start()

    # ── 窗口初始化 ─────────────────────────────────────────────────────────

    def _setup_window(self) -> None:
        """配置透明无边框、始终置顶的宠物窗口，默认出现在屏幕右下角"""
        self.root.overrideredirect(True)
        self.root.attributes('-topmost', True)
        self.root.attributes('-transparentcolor', CHROMA_HEX)
        self.root.config(bg=CHROMA_HEX)

        sw = self.root.winfo_screenwidth()
        sh = self.root.winfo_screenheight()
        x  = sw - WINDOW_SIZE - 50
        y  = sh - WINDOW_SIZE - 80       # 为任务栏留空间
        self.root.geometry(f'{WINDOW_SIZE}x{WINDOW_SIZE}+{x}+{y}')

    # ── 路径辅助 ───────────────────────────────────────────────────────────

    def _assets_dir(self) -> str:
        """
        解析 assets/ 目录的绝对路径，兼容开发模式与 PyInstaller 打包模式。
        打包后优先使用 exe 同目录下的 assets/（方便用户替换素材）。
        """
        if getattr(sys, 'frozen', False):
            return os.path.join(os.path.dirname(sys.executable), 'assets')
        return os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            'assets',
        )

    # ── Sprite sheet 加载 ──────────────────────────────────────────────────

    def _load_sheets(self) -> None:
        """
        扫描 assets/sheets/ 中已知动画名称对应的 PNG 条带。
        成功加载的 sheet 会覆盖 self._animations 中的 Pillow 占位符。
        'slow_click' 不对应独立 sheet——它继承 'click' 的帧，但保持自身 fps。
        """
        sheets_dir = os.path.join(self._assets_dir(), 'sheets')
        # 可被 sheet 替换的动画名称（slow_click 特殊处理）
        for name in ('idle', 'click', 'drag', 'fly', 'dizzy', 'sleep'):
            anim = self._try_load_sheet(name, sheets_dir)
            if anim is None:
                continue
            self._animations[name] = anim
            if name == 'click':
                # slow_click 共享 click 的帧，但以较慢 fps 播放
                self._animations['slow_click'] = Animation(
                    anim.frames, fps=1000 / 160, loop=False,
                )

    def _try_load_sheet(self, name: str, sheets_dir: str) -> 'Optional[Animation]':
        """
        尝试从 sheets_dir/{name}.png 加载 sprite sheet。

        帧尺寸自动检测：以图像高度为边长作为正方形帧尺寸，
        帧数 = 图像宽度 // 帧高度。
        返回 Animation（已合成到色度键画布），失败时返回 None。
        """
        path = os.path.join(sheets_dir, f'{name}.png')
        if not os.path.exists(path):
            return None
        try:
            probe  = Image.open(path)
            h      = probe.height
            if h == 0:
                return None
            count  = max(1, probe.width // h)
            loop   = name not in _SHEET_NON_LOOPING
            raw    = load_sprite_sheet(path, h, h, count)
            anim   = build_animation(raw, fps=12.0, loop=loop)
            print(f'[Pet] Sheet: {name}.png  {count} frames  '
                  f'{anim.fps:.1f} fps  loop={loop}')
            return anim
        except Exception as exc:
            print(f'[Pet] Could not load sheet {name}.png: {exc}')
            return None

    # ── 公开动画 API ──────────────────────────────────────────────────────

    def play(self, animation_name: str) -> None:
        """
        切换到指定名称的动画。
        若名称不存在则静默忽略（允许在 sheet 未加载时调用 play('sleep') 而不崩溃）。
        """
        if animation_name in self._animations:
            self._switch_anim(animation_name)

    def _load_character(self) -> Image.Image:
        """
        依次查找 assets/ 中的已知文件名，找到则加载，
        否则即时生成内置占位符（不写入磁盘）。
        """
        assets = self._assets_dir()
        for fname in ('mutsumi.png', 'character.png', 'pet.png'):
            path = os.path.join(assets, fname)
            if os.path.exists(path):
                try:
                    img = Image.open(path).convert('RGBA')
                    print(f'[Pet] Loaded character: {fname}')
                    return img
                except Exception as exc:
                    print(f'[Pet] Failed to load {fname}: {exc}')

        print('[Pet] No character asset found — generating placeholder.')
        from placeholder import create_placeholder_character
        return create_placeholder_character()

    # ── 状态 tick 调度 ──────────────────────────────────────────────────────

    def _schedule_tick(self) -> None:
        """每 TICK_MS 毫秒调用一次 PetState.tick()，在主线程中执行"""
        self.root.after(TICK_MS, self._do_tick)

    def _do_tick(self) -> None:
        """tick 回调：推进宠物状态，再次调度下一个 tick"""
        self._pet_state.tick()
        self._schedule_tick()   # 链式调度（替代 recurring after）

    # ── 事件订阅回调 ──────────────────────────────────────────────────────

    def _on_mood_changed(self, old_mood: Mood, new_mood: Mood) -> None:
        """
        心情变化时触发。
        进入 BORED 时弹出 bored 气泡；进入/离开 SLEEPY 时控制 Z 粒子。
        """
        print(f'[Pet] Mood: {old_mood.value} → {new_mood.value}  '
              f'(energy={self._pet_state.energy:.1f}  '
              f'affection={self._pet_state.affection:.1f})')

        if new_mood == Mood.BORED:
            self._show_bubble('bored')

        if new_mood == Mood.SLEEPY:
            self._z_particles.start()
            if 'sleep' in self._animations:
                self._switch_anim('sleep')
        elif old_mood == Mood.SLEEPY:
            self._z_particles.stop()
            if self._anim == 'sleep':
                self._switch_anim('idle')

    # ── 动画循环 ───────────────────────────────────────────────────────────

    def _start_anim(self) -> None:
        """重置帧索引并立即启动动画循环"""
        self._frame_idx = 0
        self._animate()

    def _animate(self) -> None:
        """帧回调：更新画布图像，用 after() 调度下一帧"""
        # 飞行状态由 _fly_step() 直接驱动画布，此循环不介入
        if self._anim == 'fly':
            return

        # 待机动画被禁用时固定显示第一帧
        if self._anim == 'idle' and not self._idle_enabled:
            self.canvas.itemconfig(self._img_item, image=self.frames['idle'][0])
            return

        frames = self.frames[self._anim]
        anim   = self._animations[self._anim]
        self.canvas.itemconfig(self._img_item, image=frames[self._frame_idx])
        self._frame_idx = (self._frame_idx + 1) % len(frames)

        # 非循环动画播完一整轮后自动切回待机
        if not anim.loop and self._frame_idx == 0:
            self._anim      = 'idle'
            self._frame_idx = 0

        self._anim_job = self.root.after(
            self._animations[self._anim].interval_ms, self._animate
        )

    def _switch_anim(self, new_anim: str) -> None:
        """切换动画状态，取消当前 after 任务（含挤压弹回）再重新启动"""
        if self._ss_job is not None:
            self.root.after_cancel(self._ss_job)
            self._ss_job   = None
            self._ss_phase = ''
        if self._anim_job is not None:
            self.root.after_cancel(self._anim_job)
            self._anim_job = None
        self._anim      = new_anim
        self._frame_idx = 0
        self._animate()

    # ── 挤压弹回动画 ──────────────────────────────────────────────────────

    def _play_squash_stretch(self) -> None:
        """
        播放点击挤压弹回效果（squash → rebound → idle）。
        若正在播放中，立即重新开始（完全可打断）。
        """
        if self._ss_job is not None:
            self.root.after_cancel(self._ss_job)
            self._ss_job = None
        if self._anim_job is not None:
            self.root.after_cancel(self._anim_job)
            self._anim_job = None
        self._ss_phase = 'squash'
        self._ss_idx   = 0
        self._ss_step()

    def _ss_step(self) -> None:
        self._ss_job = None
        if self._ss_phase == 'squash':
            frames, ms = self._ss_squash, _SS_SQUASH_MS
        elif self._ss_phase == 'rebound':
            frames, ms = self._ss_rebound, _SS_REBOUND_MS
        else:
            return

        if self._ss_idx < len(frames):
            self.canvas.itemconfig(self._img_item, image=frames[self._ss_idx])
            self._ss_idx += 1
            self._ss_job = self.root.after(ms, self._ss_step)
        elif self._ss_phase == 'squash':
            # 挤压结束 → 立即进入弹回阶段
            self._ss_phase = 'rebound'
            self._ss_idx   = 0
            self._ss_job   = self.root.after(0, self._ss_step)
        else:
            # 弹回结束 → 恢复待机动画
            self._ss_phase = ''
            self._switch_anim('idle')

    # ── 鼠标事件 ───────────────────────────────────────────────────────────

    def _on_press(self, event: tk.Event) -> None:
        """记录按下时的坐标基准，为区分点击与拖拽做准备"""
        if self._flying:
            self._cancel_fly()          # 用户在飞行中抓住了宠物
        self._drag_mx  = event.x_root
        self._drag_my  = event.y_root
        self._drag_wx  = self.root.winfo_x()
        self._drag_wy  = self.root.winfo_y()
        self._dragging = False
        self._motion_samples.clear()    # 清除上次拖拽残留的采样

    def _on_motion(self, event: tk.Event) -> None:
        """超过 5 像素位移时进入拖拽模式，更新状态并移动窗口"""
        dx = event.x_root - self._drag_mx
        dy = event.y_root - self._drag_my

        if not self._dragging and (abs(dx) > 5 or abs(dy) > 5):
            self._dragging = True
            self._drag_start_time = datetime.now()  # 记录拖拽开始时间
            self.bubble.hide()              # 拖拽时隐藏已有气泡
            self._pet_state.on_drag()       # 更新宠物状态（+2 亲密度）
            self._reset_chatter_timer()     # 拖拽视为互动，重置闲聊计时
            self._switch_anim('drag')

        if self._dragging:
            # 滚动采样窗口：保留最近 5 个位置点，用于释放时计算抛投速度
            self._motion_samples.append((time.monotonic(), event.x_root, event.y_root))
            if len(self._motion_samples) > 5:
                self._motion_samples.pop(0)
            self.root.geometry(f'+{self._drag_wx + dx}+{self._drag_wy + dy}')

    def _on_release(self, event: tk.Event) -> None:
        """
        鼠标释放，分两条路径：

        拖拽结束
          计算拖拽持续时间：< 0.5 s = 粗暴（rough），否则 = 温柔（gentle）。
          粗暴拖拽：亲密度 -3，显示 dragged_rough 台词。
          温柔搬运：无额外变化，显示 dragged_gentle 台词。

        纯点击
          始终调用 on_click()（+5 亲密度）。
          连点检测：2 秒内累计 3+ 次 → 抚摸（on_pet +5，显示 petted 台词）。
          否则按当前心情选择台词和动画速度（嗜睡 → slow_click）。
        """
        if self._dragging:
            self._dragging = False
            vx, vy, speed = self._compute_throw_velocity()

            if speed > _THROW_THRESHOLD:
                # ── 抛投：释放速度过高，进入飞行状态 ──
                s = self._pet_state
                s.affection = max(0.0, min(100.0, s.affection - 5.0))
                s._recalc_mood()
                print(f'[Pet] thrown!  speed={speed:.0f} px/s  '
                      f'affection={s.affection:.1f}')
                self._start_fly(vx, vy)
            else:
                # ── 普通拖拽结束：判断粗暴 / 温柔 ──
                duration = (
                    (datetime.now() - self._drag_start_time).total_seconds()
                    if self._drag_start_time else 1.0
                )
                rough = duration < 0.5
                self._pet_state.on_drag_release(rough)
                self._switch_anim('idle')
                self._show_bubble('dragged_rough' if rough else 'dragged_gentle')
                print(f'[Pet] drag {"rough" if rough else "gentle"} '
                      f'({duration:.2f}s)  affection={self._pet_state.affection:.1f}')
        else:
            # ── 普通点击 ──
            self._pet_state.on_click()

            now = datetime.now()
            self._click_times.append(now)
            # 保留 2 秒内的点击记录，过期的直接丢弃
            self._click_times = [
                t for t in self._click_times
                if (now - t).total_seconds() < 2.0
            ]

            if len(self._click_times) >= 3:
                # ── 抚摸：快速连点 3+ 次 ──
                self._pet_state.on_pet()
                self._play_squash_stretch()
                self._show_bubble('petted')
                print(f'[Pet] petted!  affection={self._pet_state.affection:.1f}  '
                      f'mood={self._pet_state.mood.value}')
            else:
                # ── 单次点击：挤压弹回（所有心情通用） ──
                mood = self._pet_state.mood
                self._play_squash_stretch()
                self._show_bubble('clicked')   # _pick_category 内部按心情路由
                print(f'[Pet] click    affection={self._pet_state.affection:.1f}  '
                      f'mood={mood.value}')

            self._reset_chatter_timer()

    def _on_right_click(self, event: tk.Event) -> None:
        """右键弹出上下文菜单"""
        self.menu.show(event.x_root, event.y_root)

    # ── 抛投物理 ───────────────────────────────────────────────────────────

    def _compute_throw_velocity(self) -> Tuple[float, float, float]:
        """
        从最近 100 ms 内的采样点计算抛投速度，返回 (vx, vy, speed)。
        采样不足或时间间隔过短时返回零向量。
        """
        now    = time.monotonic()
        recent = [(t, x, y) for t, x, y in self._motion_samples
                  if now - t <= 0.10]
        if len(recent) >= 2:
            dt = recent[-1][0] - recent[0][0]
            if dt > 1e-4:
                vx = (recent[-1][1] - recent[0][1]) / dt
                vy = (recent[-1][2] - recent[0][2]) / dt
                return vx, vy, math.sqrt(vx * vx + vy * vy)
        return 0.0, 0.0, 0.0

    def _start_fly(self, vx: float, vy: float) -> None:
        """进入飞行状态：取消普通动画循环，启动物理步骤链"""
        if self._anim_job is not None:
            self.root.after_cancel(self._anim_job)
            self._anim_job = None
        self._anim   = 'fly'
        self._flying = True
        self._fly_vx = vx
        self._fly_vy = vy
        self._fly_x  = float(self.root.winfo_x())
        self._fly_y  = float(self.root.winfo_y())
        self.bubble.hide()
        self._fly_step()

    def _fly_step(self) -> None:
        """
        物理更新回调（~60 fps）：积分速度位移、施加重力、处理边界弹跳。
        同时根据速度方向向量选择对应的旋转帧，营造翻滚效果。
        速度低于停止阈值时调用 _land()。
        """
        self._fly_job = None
        dt = _FLY_STEP_MS / 1000.0

        # 积分：重力加速 + 位移
        self._fly_vy += _FLY_GRAVITY * dt
        self._fly_x  += self._fly_vx  * dt
        self._fly_y  += self._fly_vy  * dt

        sw = self.root.winfo_screenwidth()
        sh = self.root.winfo_screenheight()

        # 左右边界弹跳
        if self._fly_x < 0:
            self._fly_x  = 0.0
            self._fly_vx = abs(self._fly_vx) * _FLY_DAMPING
        elif self._fly_x + WINDOW_SIZE > sw:
            self._fly_x  = float(sw - WINDOW_SIZE)
            self._fly_vx = -abs(self._fly_vx) * _FLY_DAMPING

        # 上下边界弹跳（落地时附加水平摩擦）
        if self._fly_y < 0:
            self._fly_y  = 0.0
            self._fly_vy = abs(self._fly_vy) * _FLY_DAMPING
        elif self._fly_y + WINDOW_SIZE > sh:
            self._fly_y  = float(sh - WINDOW_SIZE)
            self._fly_vy = -abs(self._fly_vy) * _FLY_DAMPING
            self._fly_vx *= _FLY_DAMPING    # 地面摩擦衰减水平速度

        # 根据速度方向向量选择旋转帧（frame 0 = 向右，顺时针递增）
        n         = len(self.frames['fly'])
        angle_deg = math.degrees(math.atan2(self._fly_vy, self._fly_vx))
        fidx      = int(angle_deg * n / 360.0) % n
        self.canvas.itemconfig(self._img_item, image=self.frames['fly'][fidx])

        # 移动窗口
        self.root.geometry(f'+{int(self._fly_x)}+{int(self._fly_y)}')

        speed = math.sqrt(self._fly_vx ** 2 + self._fly_vy ** 2)
        if speed < _FLY_STOP_SPEED:
            self._land()
            return

        self._fly_job = self.root.after(_FLY_STEP_MS, self._fly_step)

    def _cancel_fly(self) -> None:
        """用户在飞行途中抓住宠物：停止物理循环，恢复正常待机状态"""
        if self._fly_job is not None:
            self.root.after_cancel(self._fly_job)
            self._fly_job = None
        self._flying = False
        self._anim   = 'idle'
        self._start_anim()

    def _land(self) -> None:
        """
        落地处理：停止物理循环，夹紧窗口位置，播放眩晕动画并显示 thrown 台词。
        亲密度惩罚已在抛出时扣除，此处仅负责视觉反馈。
        """
        self._flying  = False
        self._fly_job = None

        # 保证窗口完全在屏幕内（理论上弹跳逻辑已确保，此处兜底）
        sw = self.root.winfo_screenwidth()
        sh = self.root.winfo_screenheight()
        x  = max(0, min(int(self._fly_x), sw - WINDOW_SIZE))
        y  = max(0, min(int(self._fly_y), sh - WINDOW_SIZE))
        self.root.geometry(f'+{x}+{y}')

        self._switch_anim('dizzy')
        self._show_bubble('thrown')
        print(f'[Pet] landed.  affection={self._pet_state.affection:.1f}')

    # ── 台词气泡 ───────────────────────────────────────────────────────────

    def _bubble_anchor(self):
        """返回气泡的屏幕锚点：(水平中心 X, 头顶 Y)"""
        return (
            self.root.winfo_x() + WINDOW_SIZE // 2,
            self.root.winfo_y() + 28,      # 28 ≈ 角色头顶在窗口内的 Y 偏移
        )

    def _show_bubble(self, trigger: str) -> None:
        """
        根据触发器和当前状态选择台词分类，然后显示气泡。
        trigger: 'clicked' | 'dragged' | 'bored' | 'idle' | 时段键
        """
        category = self._pick_category(trigger)
        cx, cy   = self._bubble_anchor()
        self.bubble.show_for(category, cx, cy, duration=2500)

    def _pick_category(self, trigger: str) -> str:
        """
        trigger → lines.json category 的映射。
        _DIRECT_CATS に含まれる trigger はそのまま返す（pass-through）。
        'clicked' は心情によって _click_category() に委譲。
        その他は時刻カテゴリにフォールバック。
        """
        if trigger in _DIRECT_CATS:
            return trigger
        if trigger == 'clicked':
            return self._click_category()
        return self._time_of_day_cat()

    def _click_category(self) -> str:
        """
        点击触发时，根据当前心情选择台词分类。
          SLEEPY  → grumpy  （被吵醒，发脾气）
          BORED   → excited （终于有人理我了！）
          HAPPY / EXCITED → clicked 或时段问候（心情好，随机）
          NEUTRAL → idle    （普通搭话）
        """
        mood = self._pet_state.mood
        if mood == Mood.SLEEPY:
            return 'grumpy'
        if mood == Mood.BORED:
            return 'excited'
        if mood in (Mood.HAPPY, Mood.EXCITED):
            # 40% 时段问候让对话更自然，60% 通用点击台词
            return self._time_of_day_cat() if random.random() < 0.4 else 'clicked'
        # NEUTRAL
        return 'idle'

    @staticmethod
    def _time_of_day_cat() -> str:
        """
        根据本地时间返回对应的台词分类键。
        morning 5–11 / afternoon 11–17 / evening 17–22 / night 22–5
        """
        hour = datetime.now().hour
        if  5 <= hour < 11: return 'morning'
        if 11 <= hour < 17: return 'afternoon'
        if 17 <= hour < 22: return 'evening'
        return 'night'

    def _show_greeting(self) -> None:
        """
        应用启动后触发一次时段问候，持续 4 秒。
        由 __init__ 通过 root.after(600, ...) 延迟调用，确保窗口已渲染。
        """
        cx, cy = self._bubble_anchor()
        self.bubble.show_for(self._time_of_day_cat(), cx, cy, duration=4000)

    def _reset_chatter_timer(self) -> None:
        """
        取消当前闲聊定时器并重新调度。
        任何用户互动（点击、拖拽）都应调用此方法以推迟下一次自动台词。
        若设置中关闭了 idle_chatter，则取消后不重新调度。
        """
        if self._chatter_job is not None:
            self.root.after_cancel(self._chatter_job)
            self._chatter_job = None
        if not self._settings.idle_chatter_on:
            return
        freq_mins = self._settings.idle_chatter_freq_mins
        half_mins = max(1, freq_mins // 2)
        lo = (freq_mins - half_mins) * 60 * 1000
        hi = (freq_mins + half_mins) * 60 * 1000
        delay_ms = random.randint(lo, hi)
        self._chatter_job = self.root.after(delay_ms, self._fire_chatter)

    def _fire_chatter(self) -> None:
        """
        闲聊定时器到期后执行：
          · 若距上次互动超过 10 分钟 → 显示 bored 台词
          · 若心情为 happy / neutral    → 显示 idle 台词
          · 其余心情（sleepy/excited）  → 静默跳过本次，重新调度
        无论是否显示台词，都重新调度下一次闲聊。
        """
        s = self._pet_state
        if s.idle_seconds > _BORED_IDLE_SECS:
            self._show_bubble('bored')
        elif s.mood in (Mood.HAPPY, Mood.NEUTRAL):
            self._show_bubble('idle')
        # sleepy / excited 时静默跳过，不强行打断用户

        self._reset_chatter_timer()   # 无论如何都重新调度

    # ── 菜单回调 ───────────────────────────────────────────────────────────

    def _on_exit(self) -> None:
        """安全退出：取消事件订阅、定时器，停止番茄钟，销毁气泡与徽章，退出主循环"""
        # 退出前同步写入状态与设置（绕过防抖）
        self._persist.save_now('state.json',    self._state_dict())
        self._persist.save_now('settings.json', self._settings.to_dict())

        events.unsubscribe('mood_changed',            self._on_mood_changed)
        events.unsubscribe('pomodoro_focus_start',    self._on_pom_focus_start)
        events.unsubscribe('pomodoro_focus_complete', self._on_pom_focus_complete)
        events.unsubscribe('pomodoro_break_start',    self._on_pom_break_start)
        events.unsubscribe('pomodoro_break_complete', self._on_pom_break_complete)
        events.unsubscribe('user_idle',               self._on_user_idle)
        events.unsubscribe('user_returned',           self._on_user_returned)
        events.unsubscribe('stretch_reminder',        self._on_stretch_reminder)
        events.unsubscribe('tick',                    self._on_state_event)
        events.unsubscribe('interaction',             self._on_state_event)
        if self._chatter_job is not None:
            self.root.after_cancel(self._chatter_job)
            self._chatter_job = None
        if self._ss_job is not None:
            self.root.after_cancel(self._ss_job)
            self._ss_job = None
        if self._fly_job is not None:
            self.root.after_cancel(self._fly_job)
            self._fly_job = None
        self._z_particles.stop()
        self._pom.stop()
        self._activity.stop()
        self._hide_badge()
        self.bubble.hide()
        self.root.quit()
        self.root.destroy()

    def _on_toggle_animation(self) -> None:
        """切换待机浮动动画的启用状态"""
        self._idle_enabled = not self._idle_enabled
        if self._idle_enabled and self._anim == 'idle':
            self._start_anim()

    # ── 番茄钟控制（菜单回调） ──────────────────────────────────────────────

    def _pom_start(self) -> None:
        self._pom.start()
        self.menu.update_pom_state(self._pom.phase, self._pom.is_paused)

    def _pom_pause(self) -> None:
        self._pom.pause()
        self.menu.update_pom_state(self._pom.phase, self._pom.is_paused)

    def _pom_stop(self) -> None:
        self._pom.stop()
        self.menu.update_pom_state(self._pom.phase, self._pom.is_paused)
        self._hide_badge()

    def _pom_set_focus(self, mins: int) -> None:
        """更新专注时长预设；若计时器未运行则立即反映到下一轮"""
        self._pom.set_focus_mins(mins)
        print(f'[Pet] Pomodoro focus set to {mins} min')

    # ── 用户活动回调（菜单 & 事件） ──────────────────────────────────────

    def _on_stretch_toggle(self, enabled: bool) -> None:
        self._activity.set_stretch_reminders(enabled)
        self._settings.stretch_reminders_on = enabled
        self._settings_win.sync_stretch(enabled)
        self._persist.save('settings.json', self._settings.to_dict)

    def _on_user_idle(self) -> None:
        """用户离开超过 20 分钟：显示担心台词"""
        self._show_bubble('worried')

    def _on_user_returned(self) -> None:
        """用户回来：显示欢迎台词并重置闲聊计时"""
        self._show_bubble('welcome_back')
        self._reset_chatter_timer()

    def _on_stretch_reminder(self) -> None:
        """连续活动 45 分钟：提示拉伸"""
        self._show_bubble('stretch_reminder')

    # ── 持久化辅助 ────────────────────────────────────────────────────────

    def _state_dict(self) -> dict:
        s = self._pet_state
        return {
            'energy':              s.energy,
            'affection':           s.affection,
            'mood':                s.mood.value,
            'last_interaction_at': s.last_interaction_at.isoformat()
                                   if s.last_interaction_at else None,
        }

    def _load_state(self) -> PetState:
        data = self._persist.load('state.json', {})
        ps   = PetState()
        if data:
            try:
                ps.energy    = float(data.get('energy',    ps.energy))
                ps.affection = float(data.get('affection', ps.affection))
            except (TypeError, ValueError):
                pass
        return ps

    def _load_settings(self) -> Settings:
        data = self._persist.load('settings.json', {})
        return Settings.from_dict(data) if data else Settings()

    def _on_state_event(self, **_kwargs) -> None:
        """tick 或 interaction 事件触发防抖保存"""
        self._persist.save('state.json', self._state_dict)

    def _apply_settings(self, s: Settings) -> None:
        """将 Settings 应用到所有子系统，无需重启。"""
        self._reset_chatter_timer()
        self._pom.set_focus_mins(s.pom_focus_mins)
        self._pom.set_break_mins(s.pom_break_mins)
        self._activity.set_stretch_reminders(s.stretch_reminders_on)
        self.menu.set_stretch_enabled(s.stretch_reminders_on)

    def _on_settings_change(self, s: Settings) -> None:
        """设置窗口每次控件变动时调用；应用并防抖保存。"""
        self._apply_settings(s)
        self._persist.save('settings.json', s.to_dict)

    def _reset_pet_state(self) -> None:
        """将宠物状态重置为默认值并立即保存"""
        self._pet_state = PetState()
        self._persist.save_now('state.json', self._state_dict())
        print('[Pet] State reset to defaults.')

    # ── 番茄钟事件回调 ────────────────────────────────────────────────────

    def _on_pom_focus_start(self, remaining: int) -> None:
        self._switch_anim('click')
        self._show_bubble('focus_start')
        self.menu.update_pom_state(self._pom.phase, self._pom.is_paused)
        self._update_badge()
        print(f'[Pet] Pomodoro focus started  ({remaining // 60}m remaining)')

    def _on_pom_focus_complete(self) -> None:
        self._switch_anim('click')
        self._show_bubble('focus_done')
        print('[Pet] Pomodoro focus complete!')

    def _on_pom_break_start(self, remaining: int) -> None:
        self._switch_anim('idle')
        self._show_bubble('break_start')
        self.menu.update_pom_state(self._pom.phase, self._pom.is_paused)
        self._update_badge()
        print(f'[Pet] Pomodoro break started  ({remaining // 60}m remaining)')

    def _on_pom_break_complete(self) -> None:
        self.menu.update_pom_state(self._pom.phase, self._pom.is_paused)
        print('[Pet] Pomodoro break complete!')

    # ── 番茄钟浮动徽章 ───────────────────────────────────────────────────

    def _show_badge(self) -> None:
        """创建显示剩余时间的小型无边框浮动窗口（角色右上角）"""
        if self._badge is not None:
            return   # 已存在

        self._badge = tk.Toplevel(self.root)
        self._badge.overrideredirect(True)
        self._badge.attributes('-topmost', True)

        outer = tk.Frame(self._badge, bg='#D32F2F', padx=1, pady=1)
        outer.pack()
        self._badge_lbl = tk.Label(
            outer,
            text='00:00',
            font=('Segoe UI', 9, 'bold'),
            fg='white',
            bg='#D32F2F',
            padx=4, pady=2,
        )
        self._badge_lbl.pack()
        self._position_badge()

    def _hide_badge(self) -> None:
        """销毁徽章窗口并取消更新定时器"""
        if self._badge_job is not None:
            self.root.after_cancel(self._badge_job)
            self._badge_job = None
        if self._badge is not None:
            self._badge.destroy()
            self._badge = None
            self._badge_lbl = None

    def _position_badge(self) -> None:
        """将徽章定位到宠物窗口的右上角"""
        if self._badge is None:
            return
        self._badge.update_idletasks()
        bw = self._badge.winfo_reqwidth()
        wx = self.root.winfo_x()
        wy = self.root.winfo_y()
        self._badge.geometry(f'+{wx + WINDOW_SIZE - bw - 4}+{wy + 4}')

    def _update_badge(self) -> None:
        """刷新徽章文字，若计时器仍在运行则每秒重新调度；停止时销毁徽章"""
        if self._badge_job is not None:
            self.root.after_cancel(self._badge_job)
            self._badge_job = None

        if self._pom.is_idle:
            self._hide_badge()
            return

        # 确保徽章窗口存在（首次调用时创建）
        if self._badge is None:
            self._show_badge()

        phase_color = '#1565C0' if self._pom.phase == 'break' else '#D32F2F'
        if self._badge_lbl is not None:
            self._badge_lbl.config(text=self._pom.remaining_str, bg=phase_color)
            self._badge_lbl.master.config(bg=phase_color)   # outer frame
        self._position_badge()

        # 每秒自动刷新（只要不处于 idle 就一直跑）
        self._badge_job = self.root.after(1000, self._update_badge)

    def _on_about(self) -> None:
        """弹出模态关于对话框，展示宠物当前状态数值"""
        win = tk.Toplevel(self.root)
        win.title('About')
        win.geometry('310x200')
        win.resizable(False, False)
        win.attributes('-topmost', True)
        win.grab_set()

        s = self._pet_state
        tk.Label(win, text='🌸  Mutsumi Desktop Pet  🌸',
                 font=('Segoe UI', 13, 'bold')).pack(pady=(18, 4))
        tk.Label(win, text='Wakaba Mutsumi always by your side~',
                 font=('Segoe UI', 10)).pack()

        # 当前状态数值
        status = (f'Mood: {s.mood.value}   '
                  f'Energy: {s.energy:.0f}   '
                  f'Affection: {s.affection:.0f}')
        tk.Label(win, text=status, font=('Segoe UI', 9), fg='#555').pack(pady=4)

        tk.Label(win, text='Built with Python · tkinter · Pillow',
                 font=('Segoe UI', 9), fg='gray').pack(pady=2)
        tk.Button(win, text='  OK  ', command=win.destroy,
                  font=('Segoe UI', 10), width=8).pack(pady=10)
