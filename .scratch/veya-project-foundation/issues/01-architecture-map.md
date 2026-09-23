# 01：建立架构职责图

Status: done
Blocked by: none

## 要做什么

基于当前四个 crate 写 `docs/ARCHITECTURE.md`：平台事件进入 worker，经过 FlowEngine 与 Store 生成 UI 快照；用户命令反向进入 worker。标明数据所有权、原始事件与聚合视图的区别，以及各模块接口。记录 `app.rs` 的直接系统调用，供后续拆分处理。

## 验收

- 每条数据流能追到当前真实文件和类型；不把尚未完成的文件/图片/固定能力写成现状。
- 清楚区分 core 语义、存储、Windows 适配和 Iced 状态；不引入假想的跨平台抽象。
- 与 `veya-history-v0.2` 的后续 payload 规划不冲突。

## Implementation

2026-09-23：新增 `docs/ARCHITECTURE.md`，按当前代码记录平台事件、worker、FlowEngine、Store、UiState/Iced 的数据流，以及原始记录、聚合视图、来源置信度和粘贴触发的语义；将现有跨层调用列为后续任务。
