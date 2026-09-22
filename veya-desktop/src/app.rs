//! Iced shell: Search + History + Flow + Settings + tray wiring.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::capture::{CardView, Retention, SharedUi, UiState};
use crate::format;
use crate::tray::{self, TrayCmd};
use crate::worker::{spawn_worker, WorkerCmd, WorkerHandle};

pub struct App {
    shared: SharedUi,
    worker: WorkerHandle,
    tray_rx: Option<std::sync::mpsc::Receiver<TrayCmd>>,
    state: UiState,
    search: String,
    show_settings: bool,
    exclude_input: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    SearchChanged(String),
    Select(u32),
    RecopySelected,
    Copy(u32),
    Delete(u32),
    ExcludeSource(String),
    Unexclude(String),
    ExcludeInput(String),
    ExcludeSubmit,
    SetRetention(Retention),
    ToggleTracking,
    ClearHistory,
    ToggleSettings,
    ToggleRaw,
    OpenSource(String),
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let shared: SharedUi = Default::default();
        let worker = spawn_worker(shared.clone());
        let (tray_tx, tray_rx) = std::sync::mpsc::channel::<TrayCmd>();
        let tray_rx = tray::spawn_tray(tray_tx).ok().map(|_| tray_rx);

        if let Ok(mut guard) = shared.lock() {
            guard.retention = Retention::default();
            guard.tracking = true;
        }
        let state = shared.lock().map(|g| g.clone()).unwrap_or_default();
        (
            Self {
                state,
                shared,
                worker,
                tray_rx,
                search: String::new(),
                show_settings: false,
                exclude_input: String::new(),
            },
            Task::none(),
        )
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let ticks = iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::Tick);
        let keys = iced::keyboard::on_key_press(handle_key);
        Subscription::batch([ticks, keys])
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                let mut tray_cmds = Vec::new();
                if let Some(rx) = &self.tray_rx {
                    while let Ok(cmd) = rx.try_recv() {
                        tray_cmds.push(cmd);
                    }
                }
                for cmd in tray_cmds {
                    self.apply_tray(cmd);
                }
                if let Ok(guard) = self.shared.lock() {
                    let mut next = guard.clone();
                    next.selected = self.state.selected;
                    next.expanded_raw = self.state.expanded_raw;
                    self.state = next;
                }
                Task::none()
            }
            Message::SearchChanged(q) => {
                self.search = q;
                Task::none()
            }
            Message::Select(seq) => {
                self.state.selected = Some(seq);
                if let Ok(mut guard) = self.shared.lock() {
                    guard.selected = Some(seq);
                }
                Task::none()
            }
            Message::RecopySelected | Message::Copy(_) => {
                let seq = match message {
                    Message::Copy(s) => Some(s),
                    _ => self.state.selected,
                };
                if let Some(seq) = seq {
                    if let Some(card) = self.state.cards.iter().find(|c| c.sequence == seq) {
                        let text = card.full_content.clone();
                        let _ = self.worker.cmd_tx.send(WorkerCmd::Recopy { text });
                    }
                }
                Task::none()
            }
            Message::Delete(seq) => {
                let _ = self
                    .worker
                    .cmd_tx
                    .send(WorkerCmd::DeleteRecord { sequence: seq });
                Task::none()
            }
            Message::ExcludeSource(exe) => {
                let _ = self.worker.cmd_tx.send(WorkerCmd::ExcludeApp { exe });
                Task::none()
            }
            Message::Unexclude(exe) => {
                let _ = self.worker.cmd_tx.send(WorkerCmd::UnexcludeApp { exe });
                Task::none()
            }
            Message::ExcludeInput(v) => {
                self.exclude_input = v;
                Task::none()
            }
            Message::ExcludeSubmit => {
                let exe = self.exclude_input.trim().to_string();
                if !exe.is_empty() {
                    let _ = self.worker.cmd_tx.send(WorkerCmd::ExcludeApp { exe });
                    self.exclude_input.clear();
                }
                Task::none()
            }
            Message::SetRetention(r) => {
                let _ = self.worker.cmd_tx.send(WorkerCmd::SetRetention(r));
                self.state.retention = r;
                Task::none()
            }
            Message::ToggleTracking => {
                let next = !self.state.tracking;
                let _ = self.worker.cmd_tx.send(WorkerCmd::SetTracking(next));
                self.state.tracking = next;
                Task::none()
            }
            Message::ClearHistory => {
                let _ = self.worker.cmd_tx.send(WorkerCmd::ClearHistory);
                Task::none()
            }
            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
                Task::none()
            }
            Message::ToggleRaw => {
                let next = !self.state.expanded_raw;
                self.state.expanded_raw = next;
                let _ = self.worker.cmd_tx.send(WorkerCmd::SetExpandedRaw(next));
                Task::none()
            }
            Message::OpenSource(app) => {
                open_source(&app);
                Task::none()
            }
        }
    }

    fn apply_tray(&mut self, cmd: TrayCmd) {
        match cmd {
            TrayCmd::OpenWindow | TrayCmd::OpenSettings => {
                self.show_settings = matches!(cmd, TrayCmd::OpenSettings);
            }
            TrayCmd::Exit => {
                let _ = self.worker.cmd_tx.send(WorkerCmd::SetTracking(false));
                std::process::exit(0);
            }
            other => {
                tray::apply_tray_action(&other, &self.worker);
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let tracking_label = if self.state.tracking {
            "Tracking ON"
        } else {
            "Paused"
        };
        let header = row![
            text("Veya").size(18),
            Space::with_width(Length::Fill),
            button(text(tracking_label).size(12)).on_press(Message::ToggleTracking),
            button(text("Settings").size(12)).on_press(Message::ToggleSettings),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        let search = text_input("Search clipboard flow...", &self.search)
            .on_input(Message::SearchChanged)
            .padding(10)
            .size(14);

        let filtered = self.filtered_cards();
        let mut list = column![].spacing(8);
        for card in filtered {
            list = list.push(self.card_widget(card));
        }

        let left = column![search, scrollable(list).height(Length::Fill)]
            .spacing(10)
            .width(Length::Fill)
            .height(Length::Fill);

        let detail = self.detail_panel();
        let body = row![left, detail].spacing(12).height(Length::Fill);

        let status = text(format!(
            "{} records     Local only · {} history     {}",
            self.state.record_count,
            self.state.retention.label(),
            self.state.status_note
        ))
        .size(12);

        let mut root = column![header, body, status].spacing(10).padding(16);
        if self.show_settings {
            root = root.push(self.settings_panel());
        }

        container(root).width(Length::Fill).height(Length::Fill).into()
    }

    fn filtered_cards(&self) -> Vec<&CardView> {
        let q = self.search.trim();
        self.state
            .cards
            .iter()
            .filter(|c| {
                veya_core::match_field(
                    &c.full_content,
                    &c.source_app,
                    c.used_in_apps.iter().map(|s| s.as_str()),
                    q,
                )
                .is_some()
            })
            .collect()
    }

    fn settings_panel(&self) -> Element<'_, Message> {
        let ret = |r: Retention, current: Retention| {
            let label = r.label();
            let mark = if r == current { "●" } else { "○" };
            button(text(format!("{mark} {label}")).size(12)).on_press(Message::SetRetention(r))
        };

        let mut excluded = column![text("Never track:").size(12)].spacing(4);
        for exe in &self.state.excluded_apps {
            excluded = excluded.push(
                row![
                    text(exe.clone()).size(12),
                    button(text("Remove").size(11)).on_press(Message::Unexclude(exe.clone())),
                ]
                .spacing(8),
            );
        }
        let add = row![
            text_input("app.exe", &self.exclude_input)
                .on_input(Message::ExcludeInput)
                .on_submit(Message::ExcludeSubmit)
                .padding(6)
                .size(12),
            button(text("Add").size(12)).on_press(Message::ExcludeSubmit),
        ]
        .spacing(8);

        container(
            column![
                text("Privacy & retention").size(14),
                row![
                    button(text(if self.state.tracking {
                        "Tracking: ON"
                    } else {
                        "Tracking: PAUSED"
                    })
                    .size(12))
                    .on_press(Message::ToggleTracking),
                ],
                row![
                    ret(Retention::Day1, self.state.retention),
                    ret(Retention::Day7, self.state.retention),
                    ret(Retention::Day30, self.state.retention),
                    ret(Retention::Never, self.state.retention),
                ]
                .spacing(8),
                button(text("Clear history").size(12)).on_press(Message::ClearHistory),
                excluded,
                add,
            ]
            .spacing(10),
        )
        .padding(12)
        .width(Length::Fill)
        .into()
    }

    fn card_widget(&self, card: &CardView) -> Element<'_, Message> {
        let used = if card.has_paste_activity {
            format!("Used in {}", card.used_in_apps.join(" · "))
        } else {
            "No paste activity".to_string()
        };
        let source = format::source_label(&card.source_app, card.source_confidence);
        let title = if card.raw_count > 1 {
            format!("{}  · Copied {}×", card.content_preview, card.raw_count)
        } else {
            card.content_preview.clone()
        };

        let body = column![
            text(title).size(14),
            text(format!("{source}  ·  {}", card.time_label)).size(12),
            text(used).size(12),
        ]
        .spacing(4);

        button(body)
            .on_press(Message::Select(card.sequence))
            .width(Length::Fill)
            .padding(10)
            .style(if self.state.selected == Some(card.sequence) {
                iced::widget::button::primary
            } else {
                iced::widget::button::secondary
            })
            .into()
    }

    fn detail_panel(&self) -> Element<'_, Message> {
        let Some(seq) = self.state.selected else {
            return container(text("Select a history item to see its Flow").size(13))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(16)
                .into();
        };
        let Some(card) = self.state.cards.iter().find(|c| c.sequence == seq) else {
            return Space::new(Length::Fill, Length::Fill).into();
        };

        let source = format::source_label(&card.source_app, card.source_confidence);
        let hint = format::confidence_hint(card.source_confidence);

        let mut used = column![text("Used in").size(13)].spacing(6);
        if card.has_paste_activity {
            for u in &card.used_in {
                used = used.push(
                    text(format!(
                        "• {}   {} · {}",
                        u.target_app, u.method_label, u.time_label
                    ))
                    .size(13),
                );
            }
            used = used.push(text(card.paste_detail).size(11));
        } else {
            used = used.push(text("No paste activity").size(13));
        }

        let exe = card.source_app.split(' ').next().unwrap_or("").to_string();
        let mut actions = row![
            button(text("Copy").size(12)).on_press(Message::Copy(card.sequence)),
            button(text("Delete").size(12)).on_press(Message::Delete(card.sequence)),
            button(text(if self.state.expanded_raw {
                "Hide raw events"
            } else {
                "Raw events"
            })
            .size(12))
            .on_press(Message::ToggleRaw),
        ]
        .spacing(8);
        if !exe.is_empty() {
            actions = actions.push(
                button(text("Exclude this app").size(12)).on_press(Message::ExcludeSource(exe.clone())),
            );
            actions =
                actions.push(button(text("Open source").size(12)).on_press(Message::OpenSource(exe)));
        }

        let mut detail = column![
            text("Copied from").size(13),
            text(source).size(16),
            text(card.time_label.clone()).size(12),
        ]
        .spacing(4);

        if !hint.is_empty() {
            detail = detail.push(text(hint).size(11));
        }
        detail = detail.push(Space::new(Length::Shrink, 12.0));
        detail = detail.push(used);

        if self.state.expanded_raw && card.raw_count > 1 {
            let mut raw = column![text("Raw clipboard events").size(12)].spacing(4);
            for s in &card.raw_sequences {
                raw = raw.push(text(format!("#{s}")).size(11));
            }
            detail = detail.push(Space::new(Length::Shrink, 8.0));
            detail = detail.push(raw);
        }

        detail = detail.push(Space::new(Length::Shrink, 12.0));
        detail = detail.push(actions);

        container(detail)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16)
            .into()
    }
}

fn handle_key(key: Key, _mods: Modifiers) -> Option<Message> {
    match key {
        Key::Named(Named::Enter) => Some(Message::RecopySelected),
        _ => None,
    }
}

fn open_source(exe: &str) {
    #[cfg(windows)]
    {
        use std::process::Command;
        // Launch the source application by exe name (shell resolve on PATH/registered).
        let _ = Command::new("cmd").args(["/C", "start", "", exe]).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
    }
}
