# 02：历史 metadata 投影与查询游标

Status: ready-for-agent
Priority: P1
Blocked by: 01
Implementation: 规划，未实施

## 根因与负责范围

`worker.rs::load_history_page` 每页从 `cursor = None` 扫描，按 `page * HISTORY_PAGE_SIZE` 跳过匹配卡片；`Store::load_records_page` 的 SELECT 同时读取 `payload` PNG BLOB。后面的页会反复搬运前面记录的图片，即使它们不显示。负责 storage 的窄 metadata 查询、worker 分页和可见页 payload 装载。

## 方案

扫描只取分组、筛选和完整搜索需要的 metadata／文本／paste 线索；页内显示与 typed replay 按原 record ID 读取完整 payload。为同一查询记录聚合边界处的分页游标；查询或数据变化时明确失效，不设无界缓存。先定义窄数据契约，不改变 core 的现有语义。

## 验收

- 固定文本、图片、文件、pin、source confidence 和长重复组的数据集，新旧排序、搜索结果、卡片代表 ID 与分页完全一致。
- 等时间戳、跨 batch 聚合、前后翻页和 mutation 后游标不漏、不重；全文搜索不截断。
- 非当前页 PNG 不经历史扫描解码／搬运；原图展开和 replay 仍可取得完整 payload。
- 实测后页翻页的读取量、峰值内存和时间改善；首次打开及搜索无体验回退。

## Comments

05 在本票投影契约明确后实施；不先建通用查询或缓存框架。
