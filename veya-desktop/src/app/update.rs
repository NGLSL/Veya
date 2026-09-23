use std::sync::mpsc::{self, Receiver, TryRecvError};

use update::{CheckResult, InstallerAsset};
use veya_windows::platform::{shell, update};

enum Event {
    Checked(Result<CheckResult, String>),
    InstallerLaunched(Result<(), String>),
}

pub(super) struct UpdateCheck {
    pub status: Status,
    pub installer: Option<InstallerAsset>,
    receiver: Option<Receiver<Event>>,
}

#[derive(Default)]
pub(super) enum Status {
    #[default]
    Idle,
    Checking,
    NoRelease,
    UpToDate,
    Available(String),
    Downloading,
    InstallerLaunched,
    Failed(String),
}

impl Default for UpdateCheck {
    fn default() -> Self {
        Self {
            status: Status::Idle,
            installer: None,
            receiver: None,
        }
    }
}

impl UpdateCheck {
    pub fn start(&mut self) {
        if self.receiver.is_some() {
            return;
        }
        self.status = Status::Checking;
        self.installer = None;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            let result = update::check_latest(env!("CARGO_PKG_VERSION"));
            let _ = sender.send(Event::Checked(result));
        });
    }

    pub fn download_and_install(&mut self) {
        if self.receiver.is_some() || !matches!(self.status, Status::Available(_)) {
            return;
        }
        let Some(asset) = self.installer.clone() else {
            return;
        };
        self.status = Status::Downloading;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        std::thread::spawn(move || {
            let result =
                update::download_verified(&asset).and_then(|path| shell::launch_installer(&path));
            let _ = sender.send(Event::InstallerLaunched(result));
        });
    }

    pub fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        match receiver.try_recv() {
            Ok(Event::Checked(Ok(CheckResult::NoRelease))) => self.status = Status::NoRelease,
            Ok(Event::Checked(Ok(CheckResult::UpToDate))) => self.status = Status::UpToDate,
            Ok(Event::Checked(Ok(CheckResult::Available { tag, installer }))) => {
                self.installer = installer;
                self.status = Status::Available(tag);
            }
            Ok(Event::Checked(Err(error))) | Ok(Event::InstallerLaunched(Err(error))) => {
                self.status = Status::Failed(error);
            }
            Ok(Event::InstallerLaunched(Ok(()))) => self.status = Status::InstallerLaunched,
            Err(TryRecvError::Disconnected) => self.status = Status::Failed("更新操作中断".into()),
            Err(TryRecvError::Empty) => return,
        }
        self.receiver = None;
    }

    pub fn description(&self) -> String {
        match &self.status {
            Status::Idle => "手动对比 GitHub 最新发布版本".into(),
            Status::Checking => "正在检查…".into(),
            Status::NoRelease => "仓库尚无正式发布版本".into(),
            Status::UpToDate => "当前已是最新发布版本".into(),
            Status::Available(tag) => format!("发现新版本 {tag}"),
            Status::Downloading => "正在下载并校验安装包…".into(),
            Status::InstallerLaunched => "安装器已打开，请按提示完成安装".into(),
            Status::Failed(error) => format!("更新失败：{error}"),
        }
    }
}
