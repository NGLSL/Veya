# 隐藏窗口 CPU 空转验收

日期：2026-10-01。Windows 11 教育版 10.0.26200，1920×1080、100% 缩放，16 个逻辑处理器，MSVC、默认 wgpu。以下百分比均按全机逻辑核数归一，不是单核占用。

## 安装构建核对

用户安装的 `D:\Program Files\Veya\veya.exe` 与当天 `target/release-package/release/veya.exe` 完全一致，SHA-256 `40DD67880E01F39A40E805857BE7D91F91338BE6A4EF6C2832E5E6A5B811D4C9`，长度 18,435,584 bytes；文件版本资源仍为 0.2.1。最初仅据版本资源判为旧安装版的说法已更正。

安装进程 PID 35496 隐藏时连续三段 4 秒采样 CPU 6.054／6.054／6.128%，约一核 97%–98%。用户重启后 PID 27940 同样复现，三段 2 秒为 6.152／6.053／6.054%。同一进程状态对照：隐藏 6.054%，Alt+V 显示 0.260%，再次隐藏 5.924%。

热线程只有 UI 线程消耗显著 CPU。`cpu_probe.py` 使用短暂 Suspend/GetThreadContext/Resume 的 RIP 采样和本地匹配 PDB 解析，100 次中 69 次在 `NtUserRedrawWindow`，其余主要在 Iced/winit futures 和窗口事件路径。这不是 WPA/完整调用栈采样，不能据此给出各函数耗时占比。单实例监听使用自动复位事件和无限等待；平台消息泵、数据库和 capture worker 等待路径不符合此空转现象。

## 根因和修补

Iced 0.13.0 的 `ResumeTimeReached` 请求重绘，后续 `RedrawRequested` 才更换 `WaitUntil`。隐藏状态缺少后续重绘回执时，旧的过期时间会不断产生到期事件。空历史库也能复现；临时将隐藏界面替换成空控件没有解决，该实验改动已撤销。

保留上游 `iced_winit` 0.13.0 源码及 MIT license，在 Windows 的 Init／ResumeTimeReached 分支重绘前发 `ChangeFlow(Wait)`，清退已到期等待。原 runner 的未来 deadline 保护保持不变；可见窗口的重绘继续建立下一个动画时间。唯一源码改动和维护约束在 `vendor/iced_winit/PATCHES.md`。没有改变 UI 轮询、渲染器、剪贴板事件或数据库格式。

## CPU 回归

`cpu_runtime.py <binary> --output <json>` 使用唯一空 APPDATA，确认自己的 PID/HWND，实际 Alt+V 切入快捷面板后再次收起。稳定 1 秒后采样三段，各约 2 秒，每 100ms 验证窗口隐藏；任何一段 CPU >= 1% 即失败。候选和夹具均由脚本创建，安装实例由主执行者按精确 PID/路径临时暂停和恢复。

| 构建 | 三段隐藏 CPU | 判定 |
| --- | --- | --- |
| 修改前开发构建 | 1.942／0.583／0.416% | 红 |
| 空控件实验开发构建 | 2.251／2.132／5.887% | 红，已撤销 |
| runtime 修补开发构建 | 0.094／0／0.048% | 绿 |
| 修改前 release，等于用户安装文件 | 0.714／2.260／5.965% | 红 |
| runtime 修补 release | 0.047／0.097／0.048% | 绿 |
| runtime 修补 release，重复运行 | 0／0.097／0.145% | 绿 |

原始数据 `cpu-before.json`、`cpu-empty-view.json`、`cpu-fixed-debug.json`、`cpu-release-before.json`、`cpu-fixed-release.json`。短时基线的占用爬升与动画期限有关，不能用其中一个较低采样判断已消失。1% 是当前 16 核环境下捕获一核空转的回归阈值，不是所有硬件的待机承诺。

## 可见动画与恢复

最终 release 的 `cpu_behavior.py` 检查通过：41 条隔离历史，搜索框静置 3.02 秒，29 帧得到两种重复画面、5 次约 500ms 切换，差异集中在单个 x 列的 19 像素光标。主执行者复核了两张局部 PNG，光标显示／隐藏正确；没有用停止光标动画降低 CPU。

Esc 后收起并恢复宿主前台和 EDIT 焦点；再次 Alt+V 搜索 Enter，原生 EDIT 逐字等于 `CPU:OUTSIDE_FIRST_PAGE_UNIQUE`，窗口隐藏、焦点恢复，raw count 41 → 41。原始数据 `cpu-behavior-release-fast.json`，图片 `appdata-cpu-behavior-enp_vyx6-caret-0.png`／`-1.png`。最初逐像素 GDI 采样耗时过长而未形成有效动画证据，该次失败未用于验收；探针已改为每帧一次 BitBlt。

## 构建、检查与交付

修补开发构建 SHA-256 `F0FC3AD1EB16A33CB841C23F93E8178884BA2B5207B2C665C4507704EBF5DABB`；修补 release SHA-256 `FDF2938EA09B68832BF29669AE4463A77684E1CA124114945C6D80625C03F412`。

`cargo fmt --all -- --check`、`cargo test --workspace`（115 项）、`cargo check --workspace`、开发及 `--locked --release` 构建、GUI subsystem 检查和 `git diff --check` 通过。路径依赖暴露上游原有两处 deprecated `try_next` 警告，保留原代码，没有编译错误。

`scripts/build-installer.ps1` 已生成本地 `artifacts/veya-setup.exe`，7,452,279 bytes，SHA-256 `80EB441D0B474E156FC7E3B18276A762EA64CB3CB91D79DA2CCB1F62309257E7`。版本资源仍为 0.2.1，这是本地修补包；没有推送、标签、GitHub 发布或覆盖用户安装文件，也没有做修补包的真实覆盖安装／卸载验收。

候选与测试宿主全部退出；最后恢复原安装实例 PID 18196，精确路径核对后用实际 Alt+V 收起到托盘。日常历史数据库和安装文件没有修改。没有做长时间曲线、其他硬件／驱动、多显示器或多窗口性能承诺。
