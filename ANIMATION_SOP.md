# Animation Production SOP

Standard procedure for producing a new animation clip (e.g. `click`, `drag`, `sleep`) for the Mutsumi desktop pet, from AI-generated video to playback-ready frames.

## Workflow at a glance

1. **Generate** a green-screen movement video with an AI video generator, pinning the **end frame** (and optionally the start frame).
2. **Extract** frames from the video with ffmpeg.
3. **Preprocess** the frames with `normalize_frames.py` (trim + size alignment) and, if needed, `color_match_frames.py` (color tone match).

---

## Step 1 — Generate the video

| Item | Requirement |
|---|---|
| Tool | Any AI video generator that supports start/end-frame conditioning (Runway, Kling, Luma, etc.). |
| End frame | **Required.** Use the canonical neutral pose so every clip ends where idle starts. |
| Start frame | Optional. Provide it when the action needs a specific entry pose; otherwise let the AI improvise the lead-in. |
| Background | Solid green (`#00FF00`) for chroma keying. |
| Duration | ~2 seconds (≈48 frames at 24 fps) for reactions. Longer for ambient loops. |
| Output | `videos/<name>.mp4` |

---

## Step 2 — Extract frames with ffmpeg

Run chroma key + frame extraction in one pass:

```bash
ffmpeg -i videos/<name>.mp4 \
  -vf "chromakey=0x00ff00:0.10:0.05,format=rgba" \
  -r 24 \
  frames_raw/<name>/frame_%03d.webp
```

- `-r 24` — extract at 24 fps. Must match the playback fps in `pet.py`.
- `chromakey=0x00ff00:0.10:0.05` — keys out green. Increase the first tolerance if green fringes remain; increase the second to soften edges.
- Output goes to `frames_raw/<name>/`.

> Use the **same** ffmpeg command for every clip. Variations in tolerance produce visibly different edge softness.

---

## Step 3 — Preprocess

### 3a. Normalize

Trim unstable boundary frames, align character size/position, re-encode as WebP q25:

```bash
python normalize_frames.py \
  --src frames_raw/<name> \
  --dst assets/<name> \
  --trim-head 3 --trim-tail 3
```

**Locked parameters (must be identical across every clip):**

| Flag | Locked value | Meaning |
|---|---|---|
| `--canvas-size`    | `720x1280` *(default)* | output canvas dimensions |
| `--target-height`  | `800` *(default)*      | character height after rescaling |
| `--anchor-y-ratio` | `0.9` *(default)*      | character's feet position as a fraction of canvas height |

These three parameters are what guarantee cross-clip alignment — **never change them between actions** unless re-normalizing every existing clip.

**Per-clip parameters (tune by inspection):**

| Flag | Meaning |
|---|---|
| `--trim-head` | frames to drop from the START of the sequence |
| `--trim-tail` | frames to drop from the END of the sequence |

Inspect the first/last 3–5 frames manually and pick values that drop the unstable ones. Reference values used so far:

| Clip | `--trim-head` | `--trim-tail` |
|---|---|---|
| idle  | 3 | 3 |
| click | 0 | 3 |

### 3b. Color match *(only if visible color drift from idle)*

```bash
python color_match_frames.py \
  --src assets/<name> \
  --dst assets/<name>_matched \
  --reference assets/idle/frame_015.webp
```

- Always use the same mid-sequence idle frame as reference (e.g. `frame_015`). Never use `frame_001` — it may still be slightly off after trimming.

---

## Step 4 — Register in code

Add the new animation to `_load_all_animations()` in [pet.py](src/pet.py):

```python
'<name>': self._load_dir_anim('<name>_matched', fps=24.0, loop=False),
```

(Drop `_matched` if step 3b was skipped. Set `loop=True` for ambient animations.)

---

## Recipe file (recommended)

Keep a `.yaml` next to each clip for reproducibility:

```yaml
name:         click
prompt:       "character looks up, surprised reaction, returns to rest"
ai_tool:      Kling 2.1
start_frame:  keyframes/surprised.png
end_frame:    keyframes/neutral.png
fps:          24
trim_head:    0
trim_tail:    3
color_ref:    assets/idle/frame_015.webp
loop:         false
```

---

## Quality checklist before commit

- [ ] First frame of `assets/<name>/` is stable (not blurry, not partial)
- [ ] Last frame matches the canonical end pose
- [ ] All canvas/anchor parameters identical to existing clips
- [ ] Animation plays smoothly in isolation (`python src/main.py`)
- [ ] Transition into and out of idle has no visible jump
- [ ] `frames_raw/<name>/` originals committed or backed up

---
---

# 桌面宠物动画制作 SOP

为 Mutsumi 桌面宠物制作一段新动画（如 `click`、`drag`、`sleep`）的标准流程：从 AI 生成视频到可播放的帧序列。

## 流程总览

1. **生成**：用 AI 视频生成器制作绿幕动作视频，**指定结束帧**（起始帧可选）。
2. **抽帧**：用 ffmpeg 从视频中导出单帧。
3. **预处理**：用 `normalize_frames.py` 做修剪 + 尺寸对齐；必要时用 `color_match_frames.py` 做色调匹配。

---

## 第 1 步 — 生成视频

| 项 | 要求 |
|---|---|
| 工具 | 任何支持首/末帧约束的 AI 视频生成器（Runway、Kling、Luma 等）。 |
| 结束帧 | **必填。** 使用统一的"中性姿势"关键帧，保证每段动画都结束在 idle 的起点。 |
| 起始帧 | 可选。如果动作需要特定入场姿势（如 click 的惊讶表情）则提供；否则让 AI 自由发挥。 |
| 背景 | 纯绿色（`#00FF00`），用于色键抠像。 |
| 时长 | 反应类约 2 秒（24 fps 下约 48 帧）；环境循环可更长。 |
| 输出 | `videos/<name>.mp4` |

---

## 第 2 步 — 用 ffmpeg 抽帧

色键 + 抽帧一步完成：

```bash
ffmpeg -i videos/<name>.mp4 \
  -vf "chromakey=0x00ff00:0.10:0.05,format=rgba" \
  -r 24 \
  frames_raw/<name>/frame_%03d.webp
```

- `-r 24`：以 24 fps 抽帧，必须与 `pet.py` 中的播放帧率一致。
- `chromakey=0x00ff00:0.10:0.05`：去除绿背景。如果仍有绿边，增大第一个容差；想让边缘更柔和则增大第二个。
- 输出落到 `frames_raw/<name>/`。

> 所有动画**必须使用同一条 ffmpeg 命令**，容差差异会导致边缘软硬度肉眼可见的不同。

---

## 第 3 步 — 预处理

### 3a. 归一化

修剪边界不稳定帧 + 角色尺寸与位置对齐 + 以 WebP q25 重新编码：

```bash
python normalize_frames.py \
  --src frames_raw/<name> \
  --dst assets/<name> \
  --trim-head 3 --trim-tail 3
```

**锁定参数（所有动画必须完全一致）：**

| 参数 | 锁定值 | 含义 |
|---|---|---|
| `--canvas-size`    | `720x1280` *（默认）* | 输出画布尺寸 |
| `--target-height`  | `800` *（默认）*      | 角色缩放后的像素高度 |
| `--anchor-y-ratio` | `0.9` *（默认）*      | 角色脚底位置（占画布高度的比例） |

这三项是跨片段对齐的保证，**绝不能在不同动画间改动**，除非愿意把所有已有动画重新归一化。

**逐片段参数（按观察调整）：**

| 参数 | 含义 |
|---|---|
| `--trim-head` | 从序列开头丢弃的帧数 |
| `--trim-tail` | 从序列结尾丢弃的帧数 |

请手动检查首尾 3–5 帧，挑出不稳定的丢掉。当前已用的参考值：

| 动画 | `--trim-head` | `--trim-tail` |
|---|---|---|
| idle  | 3 | 3 |
| click | 0 | 3 |

### 3b. 色调匹配 *（仅在和 idle 比有明显色偏时使用）*

```bash
python color_match_frames.py \
  --src assets/<name> \
  --dst assets/<name>_matched \
  --reference assets/idle/frame_015.webp
```

- 始终用同一张 idle 中段帧（如 `frame_015`）作为参考。**不要用 `frame_001`** —— 修剪后它可能仍有轻微不稳。

---

## 第 4 步 — 在代码中注册

在 [pet.py](src/pet.py) 的 `_load_all_animations()` 中加入新动画：

```python
'<name>': self._load_dir_anim('<name>_matched', fps=24.0, loop=False),
```

（如果跳过了 3b，则去掉 `_matched`；环境循环动画把 `loop` 设为 `True`。）

---

## 配方文件（建议）

为每段动画保留一份 `.yaml` 配方，方便复现：

```yaml
name:         click
prompt:       "character looks up, surprised reaction, returns to rest"
ai_tool:      Kling 2.1
start_frame:  keyframes/surprised.png
end_frame:    keyframes/neutral.png
fps:          24
trim_head:    0
trim_tail:    3
color_ref:    assets/idle/frame_015.webp
loop:         false
```

---

## 提交前质检清单

- [ ] `assets/<name>/` 的第一帧稳定（不模糊、不残缺）
- [ ] 最后一帧匹配中性结束姿势
- [ ] canvas / anchor 参数与已有动画完全一致
- [ ] 单独播放时动画流畅（`python src/main.py`）
- [ ] 与 idle 之间的双向切换没有明显跳跃
- [ ] `frames_raw/<name>/` 原始帧已提交或备份
