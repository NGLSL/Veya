# Veya Round 2 — 自动场景结果

时间：2026-09-22  
二进制：`target/release/veya.exe`  
脚本：`scripts/round2_auto.ps1`  
日志：`round2_auto.log`

## 门禁摘要

| ID | 场景 | 结果 | 关键证据 |
|----|------|------|----------|
| R2-02 | Copy A → Copy B → Paste 不串号 | **Pass** | A=#10267，B=#10279，PASTE 仅挂 B → Code.exe |
| R2-03 | 一次 COPY，Ctrl+V×10 | **Pass** | #10291 挂 10 次 PASTE，无额外 COPY |
| R2-07 | 中文/多行/Emoji | **Pass** | Text=`你好 Veya 第二行 🚀`，同 seq PASTE |

## 结论（自动层）

进程级关联模型在脚本化场景下**稳定**：

- 多次复制不会串 record
- 连续粘贴可稳定挂在同一 clipboard sequence
- CF_UNICODETEXT 对中文/Emoji/换行（PoC 内显示为空格）可用

## 已知 harness 备注（非 Veya 逻辑失败）

1. **PowerShell 5.1 + UTF-8 无 BOM 脚本**会把中文写坏 → 已改为字节构造样本  
2. **keybd_event 注入 Shift+Insert / 末次 Ctrl+V** 可能因前台窗口漂移丢目标 → 人工矩阵时重点看  
3. 剪贴板被占用时 `Set-Clipboard` 会失败 → 重跑前确认无其它进程锁剪贴板

## 你接下来要做的（人工矩阵）

见 `index.html` 或下文「手动验收指引」。自动项已可在 App 中预填为 Pass。
