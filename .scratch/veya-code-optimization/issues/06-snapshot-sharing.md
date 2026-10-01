# 06：UI 快照共享当前页

Status: ready-for-human
Priority: P2
Blocked by: 01
Implementation: Arc 共享已实施；自动释放验证与 Windows 单次资源测量通过

## 负责范围与当前实现

`HistoryPage.cards` 及 `UiState.cards` 使用 `Arc<[CardView]>`，`publish` 用 `Arc::clone` 分享 immutable 当前页，减少每次状态更新复制完整文本和卡片。此票是 01 的跨快照验收，不单独增加缓存层或通用状态框架。

## 验收

- 同一查询且数据未变时多个状态快照共享当前页；更新页生成新快照，旧快照不被原地修改。
- 最新 UI revision、选中项、图片 handle 和 modal 的生命周期正确，隐藏后没有旧快照或 modal 无限保留图片。
- 查询／页变化时历史页数量有界；大文本状态更新不再逐条 clone 内容。
- 与 01 共用定向测试和 Windows 测量，不机械重复全量检查。不得仅凭引用计数变化声称 private bytes 已下降。

## Comments

本轮已完成 Arc 改动，并与 01 共用隔离 Windows 内存对照。长文本与状态切换、隐藏后的具体采样见 [运行报告](../../veya-quick-paste/validation/report.md)；不把单次测量推为长期或所有机器的保证。
