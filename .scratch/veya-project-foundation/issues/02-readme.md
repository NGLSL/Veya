# 02：补齐公开 README

Status: done
Blocked by: 01

## 要做什么

参照 Kite 的用户入口写法，为 Veya 增加根 `README.md`：一句话定位、当前功能、真实界面截图、Windows/MSVC 要求、从源码构建/运行、数据目录、隐私与限制、贡献文档链接。截图和安装说明必须来自实际可用的产物。

## 验收

- 文本、链接、代码与来源/使用记录描述准确；文件、图片、固定等未交付能力不作为已发布功能。
- 构建命令在当前 workspace 可运行；不出现本机绝对路径、虚构下载地址、许可证或安装包。
- 新人从 README 能找到架构和开发规范。

## Implementation

- Added the root `README.md` with current Windows/Rust/Iced capabilities, explicit text-only limitations, local SQLite data path, privacy behavior, build/test commands, and links to the architecture and development documents.
- Did not add a screenshot or installation/download claim because the repository has no committed verified runtime screenshot or installer artifact. This keeps the public README factual.

## Verification

- Cross-checked package names and workspace commands against the root and crate `Cargo.toml` files.
- Cross-checked feature and limitation descriptions against `veya-desktop/src/app.rs`, `veya-desktop/src/worker.rs`, `veya-desktop/src/capture.rs`, `veya-core`, `veya-storage`, and `veya-windows`.
