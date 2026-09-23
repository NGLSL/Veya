# 03：精炼 AGENTS 与开发规范

Status: done
Blocked by: 01

## 要做什么

把根 `AGENTS.md` 扩展成简短、可执行的模块地图与不变量入口；新增 `docs/DEVELOPMENT.md` 记录环境、命令、测试数据隔离、Windows 窗口/剪贴板/托盘验收、格式检查、变更范围与提交说明。保留 `docs/agents/` 的 issue 规则并使用链接引用。

## 验收

- 规则说明何时跑 core 测试、存储测试和实际 Windows UI 验证，不以 `cargo check` 代替交互验收。
- 不要求个人机器路径、特定未配置的 remote/branch、尚不存在的安装器或发布 workflow。
- README、AGENTS、DEVELOPMENT 与 ARCHITECTURE 的职责不重复冲突。

## Implementation

2026-09-23：精简根 `AGENTS.md` 为入口、模块边界、领域不变量和变更循环；新增 `docs/DEVELOPMENT.md`，记录 MSVC 环境、workspace 检查、隔离 `APPDATA` 运行、Windows 手动验收、按改动选择验证及当前未配置的发布边界。保留 `docs/agents/` 作为 issue、triage 和 domain 规则的来源。
