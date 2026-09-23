//! User-requested check of the latest public GitHub release.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

pub const REPOSITORY_URL: &str = "https://github.com/NGLSL/Veya";
pub const LATEST_RELEASE_URL: &str = "https://github.com/NGLSL/Veya/releases/latest";
const RELEASE_API_URL: &str = "https://api.github.com/repos/NGLSL/Veya/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    NoRelease,
    UpToDate,
    Available {
        tag: String,
        installer: Option<InstallerAsset>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallerAsset {
    url: String,
    size: u64,
    sha256: String,
}

pub fn check_latest(current: &str) -> Result<CheckResult, String> {
    let mut command = Command::new("curl.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows::Win32::System::Threading::CREATE_NO_WINDOW;
        command.creation_flags(CREATE_NO_WINDOW.0);
    }
    let output = command
        .args([
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "5",
            "--max-time",
            "10",
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "User-Agent: Veya",
            "--write-out",
            "\n%{http_code}",
            "--url",
            RELEASE_API_URL,
        ])
        .output()
        .map_err(|error| format!("无法启动版本检查：{error}"))?;
    if !output.status.success() {
        return Err("无法连接 GitHub，请检查网络后重试".into());
    }
    parse_http_response(&output.stdout, current)
}

fn parse_http_response(response: &[u8], current: &str) -> Result<CheckResult, String> {
    let separator = response
        .iter()
        .rposition(|byte| *byte == b'\n')
        .ok_or("版本检查响应不完整")?;
    let (body, status) = response.split_at(separator);
    let status = &status[1..];
    match status {
        b"200" => parse_release(body, current),
        b"404" => Ok(CheckResult::NoRelease),
        _ => Err(format!(
            "GitHub 返回 HTTP {}",
            String::from_utf8_lossy(status)
        )),
    }
}

fn parse_release(body: &[u8], current: &str) -> Result<CheckResult, String> {
    let release: serde_json::Value =
        serde_json::from_slice(body).map_err(|error| format!("版本信息无法解析：{error}"))?;
    let tag = release["tag_name"].as_str().ok_or("发布版本缺少标签")?;
    let latest = version_parts(tag).ok_or("发布版本标签无效")?;
    let installed = version_parts(current).ok_or("当前版本号无效")?;
    if latest > installed {
        Ok(CheckResult::Available {
            tag: tag.to_string(),
            installer: find_verified_installer(&release),
        })
    } else {
        Ok(CheckResult::UpToDate)
    }
}

fn find_verified_installer(release: &serde_json::Value) -> Option<InstallerAsset> {
    release["assets"].as_array()?.iter().find_map(|asset| {
        if asset["name"].as_str()? != "veya-setup.exe" {
            return None;
        }
        let url = asset["browser_download_url"].as_str()?;
        if !url.starts_with("https://github.com/NGLSL/Veya/releases/download/")
            || !url.ends_with("/veya-setup.exe")
        {
            return None;
        }
        let size = asset["size"].as_u64()?;
        if size == 0 {
            return None;
        }
        let sha256 = asset["digest"].as_str()?.strip_prefix("sha256:")?;
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        Some(InstallerAsset {
            url: url.into(),
            size,
            sha256: sha256.to_ascii_lowercase(),
        })
    })
}

fn version_parts(version: &str) -> Option<[u32; 3]> {
    let mut parts = version.strip_prefix('v').unwrap_or(version).split('.');
    let result = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    parts.next().is_none().then_some(result)
}

/// Download only the installer identified by a GitHub asset digest, then
/// verify both the size and SHA-256 before returning a path to execute.
pub fn download_verified(asset: &InstallerAsset) -> Result<PathBuf, String> {
    let directory = std::env::temp_dir().join("veya-updates");
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建更新目录：{error}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("系统时间无效：{error}"))?
        .as_nanos();
    let name = format!("veya-update-{}-{stamp}", std::process::id());
    let pending = directory.join(format!("{name}.part"));
    let ready = directory.join(format!("{name}.exe"));

    let result = (|| {
        let mut command = Command::new("curl.exe");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use windows::Win32::System::Threading::CREATE_NO_WINDOW;
            command.creation_flags(CREATE_NO_WINDOW.0);
        }
        let status = command
            .args([
                "--fail",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--silent",
                "--show-error",
                "--connect-timeout",
                "10",
                "--max-time",
                "120",
                "--output",
            ])
            .arg(&pending)
            .arg("--max-filesize")
            .arg(asset.size.to_string())
            .arg("--url")
            .arg(&asset.url)
            .status()
            .map_err(|error| format!("无法启动更新下载：{error}"))?;
        if !status.success() {
            return Err(format!("安装包下载失败：{status}"));
        }
        verify_installer(&pending, asset)?;
        fs::rename(&pending, &ready).map_err(|error| format!("无法准备安装包：{error}"))?;
        Ok(ready)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

fn verify_installer(path: &Path, asset: &InstallerAsset) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| format!("安装包无法读取：{error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("安装包大小无法读取：{error}"))?
        .len();
    if size != asset.size {
        return Err("安装包大小与发布信息不符".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("安装包校验失败：{error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    if format!("{:x}", hasher.finalize()) != asset.sha256 {
        return Err("安装包 SHA-256 校验失败".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions() {
        let release = br#"{"tag_name":"v0.2.0"}"#;
        assert_eq!(
            parse_release(release, "0.1.9").unwrap(),
            CheckResult::Available {
                tag: "v0.2.0".into(),
                installer: None,
            }
        );
        assert_eq!(
            parse_release(release, "0.2.0").unwrap(),
            CheckResult::UpToDate
        );
        assert_eq!(
            parse_release(release, "0.3.0").unwrap(),
            CheckResult::UpToDate
        );
    }

    #[test]
    fn rejects_invalid_release_tags() {
        assert!(parse_release(br#"{"tag_name":"preview"}"#, "0.1.0").is_err());
        assert!(parse_release(br#"{}"#, "0.1.0").is_err());
    }

    #[test]
    fn distinguishes_no_release_from_network_or_api_errors() {
        assert_eq!(
            parse_http_response(b"{\"message\":\"Not Found\"}\n404", "0.1.0").unwrap(),
            CheckResult::NoRelease
        );
        assert!(parse_http_response(b"{}\n500", "0.1.0").is_err());
        assert!(parse_http_response(b"broken response", "0.1.0").is_err());
    }

    #[test]
    fn accepts_only_installer_with_valid_github_digest() {
        let digest = format!("sha256:{:x}", Sha256::digest(b"payload"));
        let release = format!(
            r#"{{"tag_name":"v0.2.0","assets":[{{"name":"veya-setup.exe","browser_download_url":"https://github.com/NGLSL/Veya/releases/download/v0.2.0/veya-setup.exe","size":7,"digest":"{digest}"}}]}}"#
        );
        let CheckResult::Available { installer, .. } =
            parse_release(release.as_bytes(), "0.1.0").unwrap()
        else {
            panic!("expected newer version")
        };
        let asset = installer.unwrap();
        assert_eq!(asset.size, 7);
        assert_eq!(asset.sha256, digest.trim_start_matches("sha256:"));
        assert!(verify_installer_bytes(&asset));

        let missing_digest = release.replace(&digest, "");
        assert!(matches!(
            parse_release(missing_digest.as_bytes(), "0.1.0").unwrap(),
            CheckResult::Available {
                installer: None,
                ..
            }
        ));
        let foreign_url = release.replace("github.com/NGLSL/Veya", "github.com/other/Veya");
        assert!(matches!(
            parse_release(foreign_url.as_bytes(), "0.1.0").unwrap(),
            CheckResult::Available {
                installer: None,
                ..
            }
        ));
    }

    fn verify_installer_bytes(asset: &InstallerAsset) -> bool {
        let path = std::env::temp_dir().join(format!(
            "veya-verification-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::write(&path, b"payload").unwrap();
        let valid = verify_installer(&path, asset).is_ok();
        fs::write(&path, b"corrupt").unwrap();
        let rejects_corruption = verify_installer(&path, asset).is_err();
        fs::remove_file(path).unwrap();
        valid && rejects_corruption
    }
}
