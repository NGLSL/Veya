# Veya 工程结构与协作规范规划

Status: complete
Date: 2026-09-23

## 目标

让新贡献者和 Agent 能从入口文档找到真实能力、数据流、模块职责及验证方法；降低桌面端 `app.rs` 的修改冲突和跨层调用。先把当前行为稳定下来，再做保持行为不变的拆分。

## 当前事实

- Rust 2021 workspace 有 `veya-core`、`veya-storage`、`veya-windows`、`veya-desktop` 四个 crate。`veya-core` 已承载事件、FlowEngine、聚合和搜索；存储及 Win32 分别在对应 crate。
- `veya-desktop/src/app.rs` 约 2100 行，同时处理 Iced 消息、历史列表、详情、设置、格式辅助及部分系统动作。`worker.rs` 约 270 行，负责平台事件、FlowEngine、SQLite 与 UI 快照的编排。
- 根目录已有简短 `AGENTS.md` 和 `docs/agents/` 本地 issue 规则；没有根 `README.md`、CI workflow、安装脚本或 Git remote。当前只有本地 `master` 分支。不要把 Kite 的发布、安装和分支规则写成 Veya 已有能力。
- 当前剪贴板持久化以文本为主；文件、图片、固定记录在 `.scratch/veya-history-v0.2/` 单独规划。用户文档必须区分已实现和计划中能力。

## 目标职责

| 模块 | 负责 | 不负责 |
| --- | --- | --- |
| `veya-core` | 剪贴板事件、来源/去向置信度、FlowEngine、聚合及搜索语义 | Iced、SQLite、Win32 |
| `veya-storage` | SQLite schema、迁移、记录/设置读写、保留期持久化 | 事件判断、UI 展示 |
| `veya-windows` | 剪贴板/键盘/窗口/托盘/图标/单例等 Windows 行为 | 历史卡片排序、UI 状态 |
| `veya-desktop/src/worker.rs` | 将平台事件和用户命令编排成 core/storage 操作，再发布 UI 快照 | 具体 Iced 布局 |
| `veya-desktop/src/app/` | Iced 状态与消息路由；视图按历史、详情、设置拆文件 | Win32 实现、SQLite 查询 |
| `veya-desktop/src/main.rs` | 初始化、单例门禁、窗口装配 | 业务判断 |

依赖方向保持 `desktop -> core/storage/windows`，其中 `storage` 和 `windows` 只依赖必要的 core 类型。优先把 `app.rs` 的视图及纯展示辅助拆到 `app/` 子模块；不新增 crate 或仅为测试而设的公共 trait。`app.rs` 中直接调用的窗口样式、图标解析和外部打开动作，应在相关改动中收回 Windows 模块接口，避免在每帧渲染或每次 Tick 做系统查询。

## 文档分工

- `README.md`：给用户与新贡献者的产品说明、当前能力、界面、Windows 要求、源码构建、数据位置和隐私说明；只陈述已验证事实。
- `AGENTS.md`：简短且可执行的目录职责、领域不变量、改动边界、验证入口，链接详细文档；不用个人机器绝对路径。
- `docs/ARCHITECTURE.md`：事件流、命令流、四个 crate 的接口和数据所有权，解释原始事件与聚合视图的区别。
- `docs/DEVELOPMENT.md`：Rust/MSVC 环境、构建/测试命令、隔离 `%APPDATA%` 的 Windows 手动验收、格式与提交范围、不同改动的验证要求。
- 现有 `docs/agents/`：继续保存本地 issue/triage/domain 约定，避免在 `AGENTS.md` 重复全文。

## 顺序与验证

1. 写架构职责与公开 README，核实每条功能和命令。
2. 更新 AGENTS 与 DEVELOPMENT，把纯逻辑、Iced 交互、Win32 行为和存储迁移的验证要求分开。
3. 在弹窗和单例功能稳定后，按消息路由、历史、详情、设置拆分 `app.rs`；保持现有窗口尺寸和交互，通过现有测试及 Windows 真实窗口回归。
4. 整理当前格式基线后接入最小 CI：`cargo test --workspace`、`cargo check --workspace`、`cargo fmt --all -- --check`。引入严格 lint 前先处理已有告警。打包和正式发布流程待安装/分发方式确定后再单独设计。

验收以实际行为与文档一致为准：README 不宣传未实现功能；文档命令在干净 Windows/MSVC 环境可复现；模块拆分前后测试和关键桌面交互一致；CI 门禁不依赖本机路径或手工准备的数据库。

## 完成记录

2026-09-23：六项实施 issue 均完成。根 README、架构与开发规范已建立；桌面端历史、详情和设置视图拆入 `app/` 子模块；Windows shell、来源路径和窗口圆角调用已收拢；Windows CI 与 rustfmt 基线已建立。Workspace 格式、编译和 25 项测试通过。

Windows 隔离运行验证了 1080×700 启动窗口、圆角区域、第二次启动退出且只保留一个进程、文本记录、详细/紧凑列表、排序下拉、完整内容弹窗，以及最大化/恢复时窗口区域更新。未在本轮触发真实浏览器或来源应用打开；远端 CI 也因没有 Git remote 而尚未执行。
