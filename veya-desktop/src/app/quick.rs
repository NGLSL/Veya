//! The hotkey picker reuses bounded worker history and typed clipboard replay.
use super::*;
use iced::widget::column;

const QUICK_WIDTH: f32 = 440.0;
const QUICK_HEIGHT: f32 = 580.0;
const ROW_HEIGHT: f32 = 64.0;

pub(super) struct QuickPanel {
    target: Option<PasteTarget>,
    saved: ManagementState,
    pending: Option<u64>,
    pub(super) sending: bool,
    error: Option<String>,
}

struct ManagementState {
    search: String,
    filter: Filter,
    pinned_only: bool,
    sort_order: SortOrder,
    history_page: usize,
    selected: Option<u32>,
    size: iced::Size,
}

impl QuickPanel {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some() || self.sending
    }

    fn accepts(&self, request_id: u64) -> bool {
        self.pending == Some(request_id)
    }
}

impl App {
    pub(super) fn advance_window_generation(&self) -> u64 {
        self.window_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .wrapping_add(1)
    }

    pub(super) fn generation_is_current(&self, generation: u64) -> bool {
        self.window_generation
            .load(std::sync::atomic::Ordering::SeqCst)
            == generation
    }

    pub(super) fn active_scroll_id(&self) -> scrollable::Id {
        if self.quick.is_some() {
            quick_scroll_id()
        } else {
            history_scroll_id()
        }
    }

    pub(super) fn show_quick(&mut self, target: Option<PasteTarget>) -> Task<Message> {
        let generation = self.advance_window_generation();
        self.cancel_hotkey_recording();
        self.unload_content();
        self.unload_full_image();
        if self.quick.is_none() {
            self.quick = Some(QuickPanel {
                target,
                saved: ManagementState {
                    search: self.search.clone(),
                    filter: self.filter,
                    pinned_only: self.pinned_only,
                    sort_order: self.sort_order,
                    history_page: self.history_page,
                    selected: self.state.selected,
                    size: self.window_size,
                },
                pending: None,
                sending: false,
                error: self.last_paste_error.take(),
            });
        } else if let Some(quick) = &mut self.quick {
            quick.target = target;
            quick.pending = None;
            quick.error = None;
        }
        self.search.clear();
        self.filter = Filter::All;
        self.pinned_only = false;
        self.sort_order = SortOrder::Newest;
        self.history_page = 0;
        self.page = Page::History;
        self.detail_menu_open = false;
        self.card_menu = None;
        self.clear_history_confirmation_open = false;
        self.window_hidden = false;
        self.set_history_active(true);
        let query = self.request_history_query(true);
        #[cfg(windows)]
        let placement =
            veya_windows::platform::window_place::startup_placement(QUICK_WIDTH, QUICK_HEIGHT);
        #[cfg(not(windows))]
        let placement: Option<()> = None;
        let size = iced::Size::new(QUICK_WIDTH, QUICK_HEIGHT);
        #[cfg(windows)]
        let size = placement
            .map(|p| iced::Size::new(p.logical_size.0, p.logical_size.1))
            .unwrap_or(size);
        let window = iced::window::get_latest().then(move |id| {
            let Some(id) = id else { return Task::none() };
            #[cfg(windows)]
            let position = placement.map(|p| p.physical_position);
            #[cfg(not(windows))]
            let position = {
                let _ = placement;
                None
            };
            iced::window::get_scale_factor(id).map(move |scale| Message::QuickPositioned {
                generation,
                id,
                size,
                position: position.map(|physical| {
                    #[cfg(windows)]
                    let logical = veya_windows::platform::window_place::physical_to_window_logical(
                        physical, scale,
                    );
                    #[cfg(not(windows))]
                    let logical = (physical.0 / scale, physical.1 / scale);
                    iced::Point::new(logical.0, logical.1)
                }),
            })
        });
        Task::batch([query, window])
    }

    fn restore_management_state(&mut self) -> Option<iced::Size> {
        let quick = self.quick.take()?;
        self.search = quick.saved.search;
        self.filter = quick.saved.filter;
        self.pinned_only = quick.saved.pinned_only;
        self.sort_order = quick.saved.sort_order;
        self.history_page = quick.saved.history_page;
        self.state.selected = quick.saved.selected;
        Some(quick.saved.size)
    }

    pub(super) fn open_manager(&mut self) -> Task<Message> {
        if self.quick.as_ref().is_some_and(|quick| quick.sending) {
            return Task::none();
        }
        let size = self.restore_management_state();
        let generation = self.advance_window_generation();
        self.window_hidden = false;
        self.set_history_active(self.page == Page::History);
        let query = self.request_history_query(false);
        let window = iced::window::get_latest().then(move |id| {
            let resize = match (id, size) {
                (Some(id), Some(size)) => iced::window::resize(id, size),
                _ => Task::none(),
            };
            resize.chain(activate_window(generation))
        });
        Task::batch([query, window])
    }

    pub(super) fn dismiss_quick(&mut self, restore: bool) -> Task<Message> {
        let Some(quick) = &self.quick else {
            return Task::none();
        };
        if quick.sending {
            return Task::none();
        }
        let target = quick.target;
        let generation = self.advance_window_generation();
        let size = self.restore_management_state();
        self.window_hidden = true;
        self.set_history_active(false);
        hide_window()
            .chain(iced::window::get_latest().then(move |id| match (id, size) {
                (Some(id), Some(size)) => iced::window::resize(id, size),
                _ => Task::none(),
            }))
            .chain(Task::done(Message::QuickHidden {
                generation,
                target,
                restore,
            }))
    }

    pub(super) fn position_quick(
        &mut self,
        generation: u64,
        id: iced::window::Id,
        size: iced::Size,
        position: Option<iced::Point>,
    ) -> Task<Message> {
        if !self.generation_is_current(generation) || self.quick.is_none() || self.window_hidden {
            return Task::none();
        }
        iced::window::maximize(id, false)
            .chain(iced::window::resize(id, size))
            .chain(
                position
                    .map(|point| iced::window::move_to(id, point))
                    .unwrap_or_else(Task::none),
            )
            .chain(iced::window::change_mode(id, iced::window::Mode::Windowed))
            .chain(iced::window::minimize(id, false))
            .chain(Task::done(Message::QuickShown { generation, id }))
    }

    pub(super) fn focus_quick(&mut self, generation: u64, id: iced::window::Id) -> Task<Message> {
        if !self.generation_is_current(generation) || self.quick.is_none() || self.window_hidden {
            return if self.window_hidden {
                hide_window()
            } else {
                Task::none()
            };
        }
        self.window_id = Some(id);
        self.chrome_sync_pending = true;
        self.chrome_sync_attempts = 0;
        iced::window::gain_focus(id).chain(text_input::focus(search_input_id()))
    }

    pub(super) fn quick_hidden(
        &mut self,
        generation: u64,
        target: Option<PasteTarget>,
        restore: bool,
    ) -> Task<Message> {
        if !self.generation_is_current(generation) || !self.window_hidden || !restore {
            return Task::none();
        }
        let Some(target) = target else {
            return Task::none();
        };
        let token = self.window_generation.clone();
        Task::perform(
            run_system_action(move || {
                if token.load(std::sync::atomic::Ordering::SeqCst) != generation {
                    return Ok(());
                }
                veya_windows::platform::paste::restore_target(target)
            }),
            move |result| Message::TargetRestored { generation, result },
        )
    }

    pub(super) fn quick_paste(&mut self, sequence: u32) -> Task<Message> {
        if self.history_query_pending()
            || !self
                .state
                .cards
                .iter()
                .any(|card| card.sequence == sequence)
        {
            return Task::none();
        }
        let Some(quick) = &mut self.quick else {
            return Task::none();
        };
        if quick.busy() {
            return Task::none();
        }
        if quick.target.is_none() {
            quick.error = Some("请在需要粘贴的应用中按快捷键打开面板".into());
            return Task::none();
        }
        self.next_paste_request = self.next_paste_request.wrapping_add(1);
        let request_id = self.next_paste_request;
        quick.pending = Some(request_id);
        quick.error = None;
        if self
            .worker
            .cmd_tx
            .send(WorkerCmd::PreparePaste {
                sequence,
                request_id,
            })
            .is_err()
        {
            quick.pending = None;
            quick.error = Some("剪贴板后台已停止，请重新启动 Veya".into());
        }
        Task::none()
    }

    pub(super) fn quick_prepared(
        &mut self,
        request_id: u64,
        result: Result<u32, String>,
    ) -> Task<Message> {
        let Some(quick) = &mut self.quick else {
            return Task::none();
        };
        if !quick.accepts(request_id) || quick.sending {
            return Task::none();
        }
        match result {
            Err(error) => {
                quick.pending = None;
                quick.error = Some(error);
                Task::none()
            }
            Ok(sequence) => {
                let Some(target) = quick.target else {
                    return Task::none();
                };
                quick.sending = true;
                self.window_hidden = true;
                self.set_history_active(false);
                hide_window().chain(Task::perform(
                    run_system_action(move || {
                        veya_windows::platform::paste::paste_to_target(target, sequence)
                    }),
                    move |result| Message::QuickFinished { request_id, result },
                ))
            }
        }
    }

    pub(super) fn finish_quick_paste(
        &mut self,
        request_id: u64,
        result: Result<(), String>,
    ) -> Task<Message> {
        let Some(quick) = &mut self.quick else {
            return Task::none();
        };
        if !quick.accepts(request_id) || !quick.sending {
            return Task::none();
        }
        quick.pending = None;
        quick.sending = false;
        if let Err(error) = result {
            let target = quick.target;
            let current = veya_windows::platform::paste::capture_target();
            if current.is_some() && current != target {
                self.last_paste_error = Some(error);
                return self.dismiss_quick(false);
            }
            quick.error = Some(error);
            self.window_hidden = false;
            self.set_history_active(true);
            activate_window(
                self.window_generation
                    .load(std::sync::atomic::Ordering::SeqCst),
            )
        } else {
            let size = self.restore_management_state();
            self.advance_window_generation();
            iced::window::get_latest().then(move |id| match (id, size) {
                (Some(id), Some(size)) => iced::window::resize(id, size),
                _ => Task::none(),
            })
        }
    }

    pub(super) fn quick_move(&mut self, delta: i32) -> Task<Message> {
        if self.quick.as_ref().is_none_or(|quick| quick.busy()) || self.history_query_pending() {
            return Task::none();
        }
        let sequences: Vec<u32> = self.state.cards.iter().map(|card| card.sequence).collect();
        let Some(index) = moved_index(&sequences, self.state.selected, delta) else {
            return Task::none();
        };
        self.select_history_card(sequences[index]);
        // Each row has a fixed height; keep the keyboard selection in view.
        let viewport = (self.window_size.height - 170.0).max(ROW_HEIGHT);
        let content = sequences.len() as f32 * ROW_HEIGHT;
        let overflow = (content - viewport).max(1.0);
        let offset =
            (index as f32 * ROW_HEIGHT - viewport * 0.5 + ROW_HEIGHT * 0.5).clamp(0.0, overflow);
        scrollable::snap_to(
            quick_scroll_id(),
            scrollable::RelativeOffset {
                x: 0.0,
                y: offset / overflow,
            },
        )
    }

    pub(super) fn quick_view(&self) -> Element<'_, Message> {
        let Some(quick) = &self.quick else {
            return Space::new(0, 0).into();
        };
        let busy = quick.busy();
        let pending = self.history_query_pending();
        let header = row![
            mouse_area(
                row![
                    iced::widget::image(logo_handle()).width(22).height(22),
                    body("快捷粘贴").size(15),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            )
            .on_press(Message::WindowDrag),
            Space::with_width(Length::Fill),
            button(meta("管理历史").size(12))
                .style(theme::action_button)
                .on_press(Message::OpenManager),
            button(icons::icon(Icon::Close, theme::MUTED, 12.0))
                .style(theme::win_button)
                .on_press(Message::QuickDismiss { restore: true }),
        ]
        .align_y(Alignment::Center)
        .spacing(6);
        let search = text_input("搜索剪贴板历史…", &self.search)
            .id(search_input_id())
            .size(14)
            .padding(10)
            .style(theme::input_style);
        let search = if busy {
            search
        } else {
            search.on_input(Message::SearchChanged)
        };
        let list: Element<'_, Message> = if pending || self.state.cards.is_empty() {
            container(
                column![
                    icons::icon(Icon::History, theme::FAINT, 28.0),
                    meta(if pending {
                        "正在读取历史…"
                    } else if self.search.is_empty() {
                        "复制一些内容后，会显示在这里"
                    } else {
                        "没有匹配结果，试试其他关键词"
                    })
                    .size(13),
                ]
                .spacing(12)
                .align_x(Alignment::Center),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else {
            let mut rows = column![].spacing(0);
            for card in self.state.cards.iter() {
                let source = match card.source_confidence {
                    veya_core::SourceConfidence::Exact => short_app(&card.source_app),
                    veya_core::SourceConfidence::Likely => {
                        format!("{} · 推断", short_app(&card.source_app))
                    }
                    veya_core::SourceConfidence::Unknown => "来源未知".into(),
                };
                let mut metadata =
                    row![meta(source).size(11), meta(card.relative_time()).size(11)].spacing(8);
                if card.pinned {
                    metadata = metadata.push(icons::icon(Icon::Pin, theme::ACCENT, 11.0));
                }
                let content = row![
                    history::card_leading_visual(card, 34.0),
                    column![
                        body(compact_preview(&card.content_preview, 46))
                            .size(13)
                            .shaping(text::Shaping::Advanced),
                        metadata
                    ]
                    .spacing(5)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center);
                let row = button(content)
                    .width(Length::Fill)
                    .height(ROW_HEIGHT)
                    .padding(10)
                    .style(theme::card_button(
                        self.state.selected == Some(card.sequence),
                    ));
                let row = if busy {
                    row
                } else {
                    row.on_press(Message::QuickPaste(card.sequence))
                };
                rows = rows.push(row);
            }
            scrollable(rows)
                .id(quick_scroll_id())
                .height(Length::Fill)
                .style(theme::scroll_style)
                .into()
        };
        let footer = row![
            meta("↑ ↓ 选择 · Enter 粘贴 · Esc 返回").size(11),
            Space::with_width(Length::Fill),
            button(meta("上一页").size(11))
                .style(theme::action_button)
                .on_press_maybe(
                    (!busy && !pending && self.history_page > 0)
                        .then_some(Message::PreviousHistoryPage)
                ),
            button(meta("下一页").size(11))
                .style(theme::action_button)
                .on_press_maybe(
                    (!busy && !pending && self.state.history_has_next)
                        .then_some(Message::NextHistoryPage)
                ),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let mut layout =
            column![header, search, theme::hdivider(), list, theme::hdivider()].spacing(9);
        if let Some(error) = &quick.error {
            layout = layout.push(meta(error.clone()).size(12).color(theme::WARN));
        } else if busy {
            layout = layout.push(meta("正在粘贴，请松开按键…").size(12));
        }
        container(layout.push(footer))
            .padding(14)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::shell_style)
            .into()
    }
}

fn quick_scroll_id() -> scrollable::Id {
    scrollable::Id::new("quick-history")
}

fn moved_index(sequences: &[u32], selected: Option<u32>, delta: i32) -> Option<usize> {
    if sequences.is_empty() {
        return None;
    }
    let current = selected
        .and_then(|seq| sequences.iter().position(|candidate| *candidate == seq))
        .unwrap_or(0);
    Some((current as i64 + delta as i64).clamp(0, sequences.len() as i64 - 1) as usize)
}

async fn run_system_action(
    action: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::Builder::new()
        .name("veya-paste".into())
        .spawn(move || {
            let _ = tx.send(action());
        })
        .map_err(|error| format!("无法执行粘贴操作：{error}"))?;
    rx.await.map_err(|_| "粘贴操作已中断".to_string())?
}

fn quick_key(key: Key, modifiers: Modifiers) -> Option<Message> {
    if !modifiers.is_empty() {
        return None;
    }
    match key {
        Key::Named(Named::ArrowUp) => Some(Message::QuickMove(-1)),
        Key::Named(Named::ArrowDown) => Some(Message::QuickMove(1)),
        Key::Named(Named::Enter) => Some(Message::RecopySelected),
        Key::Named(Named::Escape) => Some(Message::QuickDismiss { restore: true }),
        _ => None,
    }
}

pub(super) fn quick_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            quick_key(key, modifiers)
        }
        iced::Event::Window(iced::window::Event::Unfocused) => {
            Some(Message::QuickDismiss { restore: false })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_app() -> App {
        let (cmd_tx, _) = std::sync::mpsc::channel();
        let (_, event_rx) = std::sync::mpsc::channel();
        let (_, activation_rx) = std::sync::mpsc::channel();
        App {
            quick: None,
            next_paste_request: 0,
            window_generation: Default::default(),
            last_paste_error: None,
            shared: Default::default(),
            worker: WorkerHandle { cmd_tx, event_rx },
            tray_rx: None,
            activation_rx,
            window_hidden: false,
            window_id: None,
            hotkey_recording: false,
            hotkey_record_error: None,
            state: UiState::default(),
            tracking_control: TrackingControl::default(),
            search: "saved search".into(),
            page: Page::History,
            filter: Filter::Image,
            pinned_only: true,
            sort_order: SortOrder::Oldest,
            history_page: 2,
            history_active: true,
            list_density: ListDensity::Detailed,
            exclude_input: String::new(),
            detail_menu_open: false,
            card_menu: None,
            cursor_position: iced::Point::ORIGIN,
            window_size: iced::Size::new(1080.0, 700.0),
            content_modal_for: None,
            requested_content: None,
            clear_history_confirmation_open: false,
            copied_tick: 0,
            show_qrcode: false,
            source_paths: HashMap::new(),
            chrome_sync_pending: false,
            chrome_sync_attempts: 0,
            updates: UpdateCheck::default(),
        }
    }

    fn ready_text_app() -> (App, std::sync::mpsc::Receiver<WorkerCmd>) {
        let mut app = isolated_app();
        app.search.clear();
        app.filter = Filter::All;
        app.pinned_only = false;
        app.sort_order = SortOrder::Newest;
        app.history_page = 0;
        app.state.history_query = app.history_query();
        app.state.history_active = true;
        let summary = veya_core::HistorySummary {
            representative: veya_core::HistoryRecord {
                sequence: 1,
                content: format!("{}TAIL", "中".repeat(1_000)),
                payload: veya_core::HistoryPayload::Text,
                content_hash: "one".into(),
                source_app: "editor.exe".into(),
                source_window: "source".into(),
                source_confidence: veya_core::SourceConfidence::Exact,
                created_at_ms: 1,
                pinned: false,
                pastes: Vec::new(),
            },
            raw_sequences: vec![1],
            used_in: Vec::new(),
            last_created_at_ms: 1,
        };
        app.state.cards = vec![crate::capture::card_view_from_summary(&summary, None)].into();
        app.state.selected = Some(1);
        let (tx, rx) = std::sync::mpsc::channel();
        app.worker.cmd_tx = tx;
        (app, rx)
    }

    #[test]
    fn plain_copy_requests_original_record_without_using_excerpt() {
        let (mut app, commands) = ready_text_app();
        assert!(!app.state.cards[0].content_excerpt.contains("TAIL"));
        let _ = app.update(Message::CopyPlainText(1));
        assert!(matches!(
            commands.try_recv(),
            Ok(WorkerCmd::RecopyPlainText { sequence: 1 })
        ));
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn detail_load_is_once_per_selection_and_rejects_late_old_content() {
        let (mut app, commands) = ready_text_app();
        app.sync_detail_content();
        assert!(matches!(commands.try_recv(), Ok(WorkerCmd::LoadContent(1))));
        app.sync_detail_content();
        assert!(commands.try_recv().is_err());
        app.state.detail_content = Some((99, std::sync::Arc::from("old tail")));
        app.sync_detail_content();
        assert!(app.full_content_for(1).is_none());
        assert!(app.state.detail_content.is_none());
        let full = format!("{}TAIL", "中".repeat(1_000));
        app.state.detail_content = Some((1, std::sync::Arc::from(full.clone())));
        assert_eq!(app.full_content_for(1), Some(full.as_str()));
        let _ = app.show_quick(None);
        assert!(app.state.detail_content.is_none());
        assert!(app.requested_content.is_none());
        app.sync_detail_content();
        assert!(!commands
            .try_iter()
            .any(|command| matches!(command, WorkerCmd::LoadContent(_))));
    }

    #[test]
    fn loading_query_cannot_copy_or_expand_previous_cards() {
        let (mut app, commands) = ready_text_app();
        app.state.history_loading = true;
        let _ = app.update(Message::CopyPlainText(1));
        let _ = app.update(Message::Copy(1));
        let _ = app.update(Message::OpenContentModal(1));
        assert!(commands.try_recv().is_err());
        assert!(app.content_modal_for.is_none());
    }

    #[test]
    fn cancelling_restores_management_query_and_ignores_late_preparation() {
        let mut app = isolated_app();
        let _ = app.show_quick(None);
        app.quick.as_mut().unwrap().pending = Some(7);
        let opening_generation = app
            .window_generation
            .load(std::sync::atomic::Ordering::SeqCst);
        let _ = app.dismiss_quick(true);
        let _ = app.quick_prepared(7, Ok(42));
        let _ = app.update(Message::WindowRestored {
            id: iced::window::Id::unique(),
            generation: opening_generation,
        });
        assert!(app.quick.is_none());
        assert!(app.window_hidden);
        assert!(!app.history_active);
        assert_eq!(app.search, "saved search");
        assert_eq!(app.filter, Filter::Image);
        assert!(app.pinned_only);
        assert_eq!(app.sort_order, SortOrder::Oldest);
        assert_eq!(app.history_page, 2);
        assert!(app.window_id.is_none());
    }

    #[test]
    fn old_window_and_restore_completions_do_not_change_a_new_picker_session() {
        let mut app = isolated_app();
        let _ = app.show_quick(None);
        let old = app
            .window_generation
            .load(std::sync::atomic::Ordering::SeqCst);
        let _ = app.dismiss_quick(true);
        let _ = app.show_quick(None);
        let _ = app.update(Message::QuickShown {
            generation: old,
            id: iced::window::Id::unique(),
        });
        let _ = app.update(Message::WindowRestored {
            generation: old,
            id: iced::window::Id::unique(),
        });
        let _ = app.update(Message::TargetRestored {
            generation: old,
            result: Err("old error".into()),
        });
        assert!(app.quick.is_some());
        assert!(!app.window_hidden);
        assert!(app.window_id.is_none());
        assert!(app.state.status_note.is_empty());
        assert!(app.quick.as_ref().unwrap().pending.is_none());
    }

    #[test]
    fn selection_stays_in_bounds_and_tracks_the_selected_sequence() {
        assert_eq!(moved_index(&[], None, 1), None);
        assert_eq!(moved_index(&[9, 6, 3], Some(6), 1), Some(2));
        assert_eq!(moved_index(&[9, 6, 3], Some(9), -1), Some(0));
        assert_eq!(moved_index(&[9, 6, 3], Some(3), 1), Some(2));
        assert_eq!(moved_index(&[9, 6, 3], Some(100), -1), Some(0));
    }

    #[test]
    fn picker_keys_preserve_typing_and_modified_shortcuts() {
        assert!(matches!(
            quick_key(Key::Named(Named::Enter), Modifiers::empty()),
            Some(Message::RecopySelected)
        ));
        assert!(matches!(
            quick_key(Key::Named(Named::Escape), Modifiers::empty()),
            Some(Message::QuickDismiss { restore: true })
        ));
        assert!(matches!(
            quick_key(Key::Named(Named::ArrowDown), Modifiers::empty()),
            Some(Message::QuickMove(1))
        ));
        assert!(quick_key(Key::Character("a".into()), Modifiers::empty()).is_none());
        assert!(quick_key(Key::Named(Named::Enter), Modifiers::CTRL).is_none());
    }

    #[test]
    fn only_the_active_copy_request_can_complete_a_paste() {
        let panel = QuickPanel {
            target: None,
            saved: ManagementState {
                search: String::new(),
                filter: Filter::All,
                pinned_only: false,
                sort_order: SortOrder::Newest,
                history_page: 0,
                selected: None,
                size: iced::Size::new(1080.0, 700.0),
            },
            pending: Some(7),
            sending: false,
            error: None,
        };
        assert!(panel.accepts(7));
        assert!(!panel.accepts(6));
        assert!(panel.busy());
    }
}
