# 01：状态发布复用当前历史页

Status: ready-for-human
Priority: P1
Blocked by: none
Implementation: 实现完成；自动测试及隔离 Windows 单次内存对照通过

## 根因与负责范围

worker 的 UI 状态发布原先随 dirty／平台事件重新读取和构建历史页，即使事件只更新状态。负责 `veya-desktop/src/worker.rs` 的私有 `HistoryPage` 与失效边界，配套快照共享见 06。

当前证据：`worker.rs` 的 `HistoryPage`／`refresh` 已区分查询变化和数据失效，`publish` 复用当前页；history inactive 时清空。当前 `app.rs` 在视图格式化时计算 relative time，避免复用缓存冻结相对时间。

## 验收

- 纯状态更新复用同一页，不重复扫描 SQLite；查询、排序、filter、pin、record／paste mutation 正确触发失效。
- 隐藏／离开历史视图释放 worker 持有的当前页；快照持有者释放后的内存回落必须实测。
- 相对时间持续更新；错误页不会静默保留旧查询结果。
- worker 定向测试覆盖复用、失效和退出释放，并记录 Windows 同库操作前后的内存／延迟。

## 验证记录

本轮 worker 自动测试通过。隔离 Windows 同夹具对照的管理首页 Private Bytes 约下降 13.2%，Working Set 约下降 21.1%；隐藏后也记录了回落。这是开发构建的单次场景证据，长期曲线、release 和已安装版本仍未验证。细节见 [运行报告](../../veya-quick-paste/validation/report.md)。

## Comments

本票是已实施的小范围优化，其余规划不据此标记完成。
