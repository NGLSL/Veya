//! Non-Windows shell-action stub so the workspace can be typechecked off-box.

pub fn open_source(_executable: &str) {}

pub fn open_link(_url: &str) {}

pub fn web_search(_query: &str) {}

pub fn launch_installer(_path: &std::path::Path) -> Result<(), String> {
    Err("仅 Windows 支持运行安装器".into())
}
