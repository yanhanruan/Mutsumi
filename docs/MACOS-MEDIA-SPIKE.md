# macOS 音频与媒体公开 API 技术 Spike

> 日期：2026-08-21
>
> 适用计划：[`MACOS-ADAPTATION-PLAN.md`](MACOS-ADAPTATION-PLAN.md) Phase 4A
>
> 产品边界：macOS 13+、官网/GitHub 直分发、不使用 `MediaRemote` 等私有 API

## 1. 结论

首版采用“音频活动动画 + 媒体能力显式降级”的方案：

- 使用公开 CoreAudio 的
  [`kAudioDevicePropertyDeviceIsRunningSomewhere`](https://developer.apple.com/documentation/coreaudio/kaudiodevicepropertydeviceisrunningsomewhere)
  读取默认输出设备是否有进程正在运行 I/O，不申请录音、屏幕录制或系统音频录制权限。
- 将 `audioActivity` 标为 `degraded`。该值不是音频振幅：静音、零样本或仅保持打开的输出流仍可能返回活动，不能描述为“当前有可听声音”。
- 跨应用 now-playing 元数据、播放/暂停、上一首/下一首和 seek 继续标为 `unavailable`。Apple 的公开
  [`MPNowPlayingInfoCenter`](https://developer.apple.com/documentation/mediaplayer/mpnowplayinginfocenter/default%28%29)
  与
  [`MPRemoteCommandCenter`](https://developer.apple.com/documentation/mediaplayer/mpremotecommandcenter)
  用于应用发布自己的播放信息并处理发给自己的远程命令，不是读取或控制其他应用的统一接口。
- 不使用私有 `MediaRemote`。也不为装饰性动画使用 CoreAudio process tap 或 ScreenCaptureKit 捕获系统音频。
- 系统音量/静音具备公开 API 路径，但支持情况取决于当前输出设备；本切片不接入 UI，`systemVolume` 继续为 `unavailable`，后续单独实现和验证。

## 2. 能力决策矩阵

| 能力 | 公开 API 与权限 | 结论 | 首版 capability |
| --- | --- | --- | --- |
| 输出 I/O 活动 | 默认输出设备的 `DeviceIsRunningSomewhere`；无需捕获权限 | 可驱动动画，但可能把静音保活流判为活动 | `degraded` |
| 可听振幅 | [CoreAudio process tap](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps) 可采样输出；创建 API 从 macOS 14.2 可用，并需要系统音频捕获用途说明/授权 | 与 macOS 13 基线冲突，权限成本对装饰动画过高 | 不采用 |
| 跨应用曲名/歌手/进度 | 无与 Windows SMTC 对等的公开全局读取接口 | 保持显式降级 | `unavailable` |
| 跨应用播放控制/seek | 公开 MediaPlayer 命令中心面向本应用自己的播放会话 | 保持显式错误，不做静默 no-op | `unavailable` |
| 系统音量/静音 | CoreAudio `kAudioDevicePropertyVolumeScalar` / `kAudioDevicePropertyMute`；属性是否存在、是否可写由设备决定 | 技术可行，需覆盖内建、HDMI、USB、蓝牙与 AirPlay 后再接 UI | 暂为 `unavailable` |

AppleScript 只能针对 Music、Spotify 等单个可脚本化应用做适配，无法统一覆盖浏览器，而且会引入 Automation 权限和按应用维护成本，因此不作为跨应用媒体后端。

## 3. Apple Silicon 本机验证

在当前交互式用户会话中，以只读 CoreAudio probe 验证：

- 默认输出设备读取成功，设备 ID 为 `71`。
- `DeviceIsRunningSomewhere = 1`，属性不可写。
- 同时默认设备 `mute = 1`，说明“正在运行 I/O”确实不等价于“用户可听到声音”。
- 进程对象列表显示一个活跃输出进程，为 Chrome 的 Audio Service；证明浏览器输出可被通用 I/O 信号覆盖，不依赖浏览器专属集成。
- 当前设备主通道音量和静音属性均存在且可写；单独的声道 1/2 音量属性不存在，证明实现不能假定固定声道布局。

这次验证没有读取音频样本、曲名、页面内容或用户媒体库，也没有触发 TCC 权限弹窗。

## 4. 兼容性预期与证据等级

| 来源 | 音频活动动画 | 元数据/控制 | 当前证据 |
| --- | --- | --- | --- |
| Chrome/浏览器视频 | 输出流运行时可检测；保活流可能延迟停止 | 不支持 | Chrome Audio Service 已真机验证 |
| Apple Music | 输出流运行时应由同一设备级 API 检测 | 不支持 | API 级预期，待实际播放验证 |
| Spotify | 输出流运行时应由同一设备级 API 检测 | 不支持 | API 级预期，待安装/实际播放验证 |
| Mutsumi 自身 TTS/网页音频 | 与其他输出进程相同，会参与活动判断 | 不适用 | API 级预期 |

## 5. 实现与验收边界

- 每 500 ms 读取一次；连续 3 秒活动后发出 `audio-started`，连续 6 秒非活动后发出 `audio-stopped`，与 Windows 现有连续性阈值一致。
- CoreAudio 查询失败时保留最后状态并重试，不把采样错误伪装成静音；日志仅记录 OSStatus 与失败次数。
- macOS 仅有音频活动能力时，前端保留宠物/扬声器动画，隐藏不可用的 now-playing 悬浮控制面板。
- 设置页以三语说明这是输出 I/O 估算，避免把 degraded 能力表述为精确播放检测。

提交前自动化应覆盖 Rust 状态机、CoreAudio 常量和前端 capability 分支；真机需执行 ignored CoreAudio 读取冒烟。

## 6. 后续验证

- macOS 13 真机运行与链接验证。
- Apple Music、Spotify、Chrome 的播放、暂停、静音和保活行为矩阵。
- 内建扬声器、3.5 mm、USB、HDMI、蓝牙和 AirPlay 输出切换。
- 睡眠/唤醒、输出设备热插拔和短暂无默认设备期间的状态恢复。
- 系统音量/静音另立切片，按设备属性存在性动态降级，不与 now-playing 会话绑定。
