# 05：core 有序聚合与长组内存

Status: ready-for-human
Priority: P2
Blocked by: 02
Implementation: core 有序聚合与后台加载调用已实现；等价测试通过

## 根因与负责范围

desktop `worker.rs::same_history_group` 重复 core 聚合判断；`load_history_page` 用 `Vec<ClipboardRecord>` 保存整组，再调用 `aggregate::history_cards`，后者还收集并排序引用。长重复组保留整组原始 payload，增加临时峰值。负责 `veya-core/src/aggregate.rs` 的窄有序聚合入口与 worker 调用。

## 契约与验收

- core 持有分组规则；有序输入路径保留跨 batch 状态，不要求 desktop 再复制同一规则。
- 新旧结果在升降序、聚合时间边界、等时间戳、source confidence、pin、typed payload、代表 ID 和 used-in 次序上等价。
- 可显示的聚合信息逐步累积，避免保留长组全部大 payload；不能丢失原记录关联或完整搜索字段。
- 复用现有聚合测试，补必要的跨 batch／长组等价检查，记录长组峰值内存和耗时。

## Comments

02 的 metadata 契约先落实；本票不将数据库或 Iced 引入 core。

## 实施与验证

`HistoryAggregator` 接收按时间／序号排序的轻量记录，在升降序中保留最早代表记录、时间正序的 raw ID、与原逻辑一致的使用记录顺序及来源置信度。后台每批 16 条扫描，分组状态跨批次保留，移除了 desktop 的重复分组判断和整组原始 payload 集合。

core 34 项测试及 workspace 115 项测试通过，覆盖等时间戳、5 秒边界、pin/source/hash 差异、2000 条长组、新旧聚合等价和跨存储批次。实际首屏与内存证据见 [运行报告](../validation/report.md)；未单独测量 2000 条重复组的 Windows 峰值，不宣称其具体降幅。
