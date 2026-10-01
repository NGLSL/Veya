# 03：粘贴线索增量持久化

Status: ready-for-agent
Priority: P1
Blocked by: none
Implementation: 规划，未实施

## 根因与负责范围

worker 的 paste 路径调用 `Store::insert_record`；该接口编码并 upsert 完整 payload，然后删除、重插该 record 的全部 `paste_trigger`。图片大、使用线索长时，单次 paste 产生无关写入和分配。负责 storage 的窄 append 接口与 worker paste 提交路径。

## 契约与验收

- 新 copy 保持完整原记录事务写入；paste 只追加本次观察的关联，保持原 ID、时间、method、confidence 和 typed payload。
- 数据库失败可见；明确 core 内存更新与数据库提交的顺序及恢复方式，不静默宣称已保存，不因重试重复插入。
- 自动检查涵盖多次 paste、图片 payload 不变、关联缺失与失败一致性；重启读取结果与现有成功路径等价。
- 同一大图片／长 paste 历史重复粘贴的数据库写入量、耗时与分配有对比证据。

## Comments

不改变「paste attempt 并非目标插入成功」的业务语义。
