# 06：建立可运行的质量门禁

Status: done
Blocked by: 03, 04

## 要做什么

先整理仓库当前 rustfmt 差异和编译告警，再增加最小 CI 运行 workspace check/test/fmt。CI 应使用标准 Windows Rust/MSVC 环境与隔离数据，不依赖开发机缓存、手工数据库或用户剪贴板内容。安装与发布流程待分发方式确定后另行规划。

## 验收

- 本地与 CI 使用相同命令，全部通过；格式基线清理作为独立、可审核改动。
- 行为测试不读写用户 `%APPDATA%\Veya`，不控制真实用户剪贴板。
- CI 文档描述与实际 workflow 一致，不提前承诺 Installer/Release。

## Implementation

2026-09-23：新增 `.github/workflows/ci.yml`，在 `windows-2022` 安装 stable MSVC 与 rustfmt，依次运行 workspace 格式、测试和编译检查；测试阶段把 `APPDATA` 指向 runner 临时目录。整理全 workspace rustfmt 基线并清除当前编译告警；`docs/DEVELOPMENT.md` 记录相同命令和数据隔离方式。

本地验证通过：`cargo fmt --all -- --check`、`cargo test --workspace`（25 项测试）和 `cargo check --workspace`。workflow 尚未在远端运行，因为当前仓库没有 Git remote。
