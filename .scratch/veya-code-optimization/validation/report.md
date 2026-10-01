# 首页优先与按需加载验收

日期：2026-10-01。Windows 11 教育版 10.0.26200，单屏 1920×1080、100% 缩放；MSVC，同一 Cargo 开发配置，默认 wgpu。全部运行数据库位于本目录 `appdata-*`，没有访问日常历史数据库。

## 实现与构建

- 基线是上一轮 dev 实现 `923d0e1` 对应的 `veya-candidate-v4.exe`，SHA-256 `B77B650140A0B71AA3B50D298723887C49C48BA7F838500BD13D21A98FE98F99`。
- 性能候选 `veya-lazy-row.exe`，SHA-256 `943151AFB7E42B08C0855702961B7765BBAB7BCB1DDF99668C28D6EB053215C3`。轻量投影、流式聚合、后台分阶段加载、512 字符摘要、单条按需全文、持久化缩略图和逐行 PNG 缩小均包含在此构建。
- 最终候选 `veya-lazy-final.exe`，SHA-256 `2D4A6900DC3B3731C03B984C9395CE19ED35A9040A6D4054957B71056FDE8609`。在性能候选基础上增加缩略图失败结束状态及其测试；性能矩阵没有对该最后的状态改动重跑，最终窗口 smoke 使用此构建。
- `png 0.18` 已是 `image` 使用的传递依赖，现在显式引用它的逐行解码接口；没有引入新的库版本。非交错 8-bit 大图的缩略图像素与原 `image::thumbnail` 路径对照完全一致；小图、16-bit、交错 PNG 沿用原解码路径。

## 内存对照

`memory_compare.py` 使用同一 100 条历史夹具：50 条约 1 MiB 文本、50 张 2048×2048 PNG，每张编码为 69,631 bytes。三个实例分别使用拷贝的数据库，缓存组从冷缓存候选库拷贝。没有强制清理工作集或改变渲染器。窗口置于前台，临时 topmost 防止截图被其他窗口遮挡；按精确 HWND 截取后复核了首页和 440×580 面板。

通过 `GetProcessMemoryInfo(PROCESS_MEMORY_COUNTERS_EX)` 读取 PrivateUsage、WorkingSetSize、PeakPagefileUsage 和 PeakWorkingSetSize；每 100ms 取样，首页显示后再采样 6 秒。表中峰值是操作系统的进程生命周期提交峰值，不是某一时刻的私有数据净增量。每种场景只运行一次，单位 MiB。

| 场景 | 前一轮基线 | 本轮首次生成缩略图 | 本轮已有缩略图缓存 |
| --- | --- | --- | --- |
| 管理首页 Private Bytes | 249.0 | 240.3 | 225.1 |
| 管理首页 Working Set | 200.5 | 192.2 | 179.6 |
| 进程提交内存峰值 | 275.4 | 248.6 | 232.0 |
| 快捷面板 Private Bytes | 250.0 | 240.4 | 225.1 |
| 隐藏后 Private Bytes | 237.9 | 240.4 | 225.1 |

首页私有内存在本次冷缓存对照下降约 3.5%，有缓存时下降约 9.6%。首次生成后隐藏的私有内存比基线高约 2.5 MiB，不能宣称所有后台场景都下降；有缓存时该场景下降。释放卡片不等同于渲染器／分配器立刻把所有已分配页面还给系统。

原始数据 `memory-guarded.json`；截图 `guarded-v4-home.png`、`guarded-rowcold-home.png`、`guarded-rowwarm-quick.png`。早期未保证前台的截图／测量不用于这一矩阵。

## 首屏时间与真实插入

`readiness_check.py` 在同一夹具上增加一条最新短文本 `VEYA_READY_TEXT_101`。用屏幕 GDI 像素检测当前窗口选中卡片的固定边框位置和颜色，每 5ms 轮询，测量到实际出现选中记录，而非空窗口显示。冷启动从创建进程前计时；Alt+V 从注入快捷键前计时。窗口保持可见，每次记录出现后立即 Enter，并从独立 Win32 EDIT 的 WM_GETTEXT 读取实际插入结果，两个构建的 6 次插入均为 `READY:VEYA_READY_TEXT_101`。

| 测量 | 基线 | 本轮 |
| --- | --- | --- |
| 冷启动到选中记录可见 | 1882ms | 1268ms |
| 管理窗口中第一次 Alt+V | 77ms | 88ms |
| 隐藏后再次 Alt+V，第一次 | 618ms | 238ms |
| 隐藏后再次 Alt+V，第二次 | 540ms | 232ms |

单次冷启动和少量重复唤起结果仅支持本次场景，不是稳定分位数或速度承诺。首次从已加载管理窗口切换面板略慢，隐藏后的再次唤起更快。原始数据 `readiness.json`。

## 完整内容与最终窗口检查

- 性能候选通过 `lazy_check.py` 在原生 EDIT 中重放约 1 MiB 的 #99 文本，实际全文逐字相等，长度 1,048,597（含 `LAZY:`），原始记录 100 → 100。数据 `appdata-lazy-row-runtime.json`。长文本在原生 EDIT 的布局耗时不作为 Veya 首屏指标。
- 为 #1 的约 1 MiB 文本尾部追加 `CONTENT_TAIL_ONLY`，实际管理搜索只显示该记录；点击“复制为纯文本”，直接读取 CF_UNICODETEXT 得到完整 1,048,609 字符及尾部，逐字等于 SQLite 原内容。数据 `ui-check.json` 和截图 `tail-search-guarded.png`、`tail-plain-copy.png`。该修改只在隔离运行库中进行。
- 最终构建 `final_windows.py`：42 条隔离记录，下一页正确显示第 2 页；搜索长文本尾部 `MODAL_TAIL`，纯文本复制逐字等于完整记录；完整内容弹窗显示正文并可关闭；连续切换查询后显示最新 `OUTSIDE_FIRST_PAGE_UNIQUE` 结果。
- 最终构建 Alt+V 搜索后 Esc，再次唤起并搜索 Enter：原生 EDIT 实际变为 `FINAL:OUTSIDE_FIRST_PAGE_UNIQUE`，窗口隐藏，前台与焦点回到宿主，raw count 仍为 42。数据 `final-windows.json`；截图 `final-next-page.png`、`final-full-modal.png`、`final-latest-search.png`。
- 冷缓存库只生成当前首页 13 张缩略图，不回填全部 50 张图片。缓存读取、哈希失效、删除级联、取消未确认旧页和旧回执拒绝由自动测试覆盖。

## 自动检查与边界

最终 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo test --workspace`、`cargo build -p veya-desktop`、`git diff --check` 通过，无编译警告。共 115 项测试：core 34、desktop 41、storage 16、windows 24。新增必要覆盖包括新旧双向聚合等价、跨批次长组、尾部全文搜索／复制、卡片先发布后生成图片、过期请求与详情拒绝、缓存磁盘重开和损坏缓存、PNG 色彩／alpha 对照及取消、缩略图失败结束。

本轮没有发布、覆盖安装、release 性能测试、长时间资源曲线或多显示器测试。后页目前仍从头扫描轻量投影，分页游标复用保留为后续计划；使用线索仍属于当前页聚合摘要，未改为独立详情查询。按需全文与原图的一次数据库读取仍在事件 worker 内，历史扫描和缩略图解码在独立线程。没有用降低图片质量、截断搜索或强制工作集清理换取数字。

测试宿主和所有候选进程均已退出。原安装实例按 PID／精确路径核对后暂停，最后恢复 `D:\Program Files\Veya\veya.exe`，PID 37196，使用原 APPDATA，并通过原 Alt+V 收起到托盘；EnumWindows 确认窗口隐藏。没有停止用户其它进程、修改其文档或日常数据库。
