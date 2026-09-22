//! Iced shell: Search + History + Flow + Settings actions.

use iced::widget::{
    button, column, container, row, scrollable, text, text_input, Space,
};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::capture::{CardView, SharedUi, UiState};
use crate::format;
use crate::worker::{spawn_worker, WorkerCmd, WorkerHandle};

pub struct App {
    shared: SharedUi,
    worker: WorkerHandle,
    state: UiState,
    search: String,
    show_settings: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    SearchChanged(String),
    Select(u32),
    Recopy(u32),
    Delete(u32),
    ExcludeSource(String),
    ToggleTracking,
    ClearHistory,
    Purge30d,
    ToggleSettings,
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let shared: SharedUi = Default::default();
        let worker = spawn_worker(shared.clone());
        if let Ok(mut guard) = shared.lock() {
            guard.retention_label = "Local only · 30 day history".into();
            guard.tracking = true;
        }
        let state = shared.lock().map(|g| g.clone()).unwrap_or_default();
        (
            Self {
                state,
                shared,
                worker,
                search: String::new(),
                show_settings: false,
            },
            Task::none(),
        )
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::Tick)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                if let Ok(guard) = self.shared.lock() {
                    let mut next = guard.clone();
                    next.selected = self.state.selected;
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
            Message::Recopy(seq) => {
                if let Some(card) = self.state.cards.iter().find(|c| c.sequence == seq) {
                    let text = card.full_content.clone();
                    let _ = self.worker.cmd_tx.send(WorkerCmd::Recopy { text });
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
            Message::Purge30d => {
                let cutoff = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0)
                    - 30 * 24 * 60 * 60 * 1000;
                let _ = self.worker.cmd_tx.send(WorkerCmd::PurgeOlderThan(cutoff));
                Task::none()
            }
            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text("Veya").size(18);
        let tracking_label = if self.state.tracking {
            "Tracking ON"
        } else {
            "Paused"
        };
        let header = row![
            title,
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

        let filtered: Vec<&CardView> =
            self.state.cards.iter().filter(|c| self.matches(c)).collect();

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

        let mut bottom = row![text(format!(
            "{} records     {}",
            self.state.record_count, self.state.retention_label
        ))
        .size(12)];
        if self.show_settings {
            bottom = bottom.push(Space::with_width(Length::Fill));
            bottom = bottom
                .push(button(text("Clear history").size(12)).on_press(Message::ClearHistory));
            bottom = bottom.push(button(text("Purge 30d+").size(12)).on_press(Message::Purge30d));
        }

        let mut root = column![header, body, bottom].spacing(10).padding(16);
        if self.show_settings {
            root = root.push(
                text("Privacy: Tracking · Clear history · Purge 30d · Exclude app (detail actions)")
                    .size(11),
            );
        }

        container(root).width(Length::Fill).height(Length::Fill).into()
    }

    fn matches(&self, c: &CardView) -> bool {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        c.content_preview.to_lowercase().contains(&q)
            || c.full_content.to_lowercase().contains(&q)
            || c.source_app.to_lowercase().contains(&q)
            || c.used_in.iter().any(|u| u.to_lowercase().contains(&q))
    }

    fn card_widget(&self, card: &CardView) -> Element<'_, Message> {
        let used = if card.has_paste_activity {
            format!("Used in {}", card.used_in.join(" · "))
        } else {
            "No paste activity".to_string()
        };
        let source = format::source_label(&card.source_app, card.source_confidence);
        let title = if card.raw_count > 1 {
            format!("{}  · ×{}", card.content_preview, card.raw_count)
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
                used = used.push(text(format!("• {u}")).size(13));
            }
            used = used.push(text("Paste trigger detected — insertion not verified").size(11));
        } else {
            used = used.push(text("No paste activity").size(13));
        }

        let mut actions = row![
            button(text("Copy").size(12)).on_press(Message::Recopy(card.sequence)),
            button(text("Delete").size(12)).on_press(Message::Delete(card.sequence)),
        ]
        .spacing(8);
        if !card.source_app.is_empty() {
            let exe = card.source_app.split(' ').next().unwrap_or("").to_string();
            if !exe.is_empty() {
                actions = actions.push(
                    button(text("Exclude this app").size(12)).on_press(Message::ExcludeSource(exe)),
                );
            }
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
        detail = detail.push(Space::new(Length::Shrink, 12.0));
        detail = detail.push(actions);

        container(detail)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16)
            .into()
    }
}
