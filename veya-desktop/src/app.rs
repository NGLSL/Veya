//! Iced shell: sidebar + search/filters + history list + flow detail.
//! Layout precisely aligns with Fig 1 UI mockup: glowing cards, deep navy theme,
//! unified vector icons, 4-action quick toolbar, dual source/flow cards,
//! and clean non-redundant detail view + modular settings panel.

use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Key, Modifiers};
use iced::widget::{
    button, column, container, mouse_area, opaque, pick_list, row, scrollable, stack, text,
    text_input, tooltip, Space,
};
use iced::{Alignment, Color, Element, Length, Subscription, Task, Theme};
use std::collections::HashMap;

use crate::capture::{CardPayloadView, CardView, Retention, SharedUi, UiState};
use crate::format::{self, ContentKind};
use crate::icons::{self, Icon};
use crate::theme::{self, body, meta, pad, section_label};
use crate::tray::{self, TrayCmd};
use crate::worker::{spawn_worker, WorkerCmd, WorkerHandle};
use veya_windows::hotkey::{Hotkey, MOD_ALT, MOD_CONTROL, MOD_SHIFT};
use veya_windows::platform::WindowSignal;

mod detail;
mod history;
mod settings;
mod tracking;
mod update;

use tracking::TrackingControl;
use update::UpdateCheck;

/// The initial window is hidden until it has moved to the cursor monitor.
fn show_initial_window(physical_position: Option<(f32, f32)>) -> Task<Message> {
    iced::window::get_latest().then(move |id| {
        let Some(id) = id else { return Task::none() };
        iced::window::get_scale_factor(id).then(move |scale| {
            let move_window = physical_position
                .map(|physical| {
                    let logical = veya_windows::platform::window_place::physical_to_window_logical(
                        physical, scale,
                    );
                    iced::window::move_to(id, iced::Point::new(logical.0, logical.1))
                })
                .unwrap_or_else(Task::none);
            move_window
                .chain(iced::window::change_mode(id, iced::window::Mode::Windowed))
                .chain(Task::done(Message::WindowReady(id)))
        })
    })
}

/// Keep the Iced window alive when closed to tray, and restore it for every
/// activation source (tray, second launch, and the global shortcut).
fn activate_window() -> Task<Message> {
    iced::window::get_latest().then(|id| {
        let Some(id) = id else { return Task::none() };
        iced::window::change_mode(id, iced::window::Mode::Windowed)
            .chain(iced::window::minimize(id, false))
            .chain(Task::done(Message::WindowRestored(id)))
    })
}

fn hide_window() -> Task<Message> {
    iced::window::get_latest().then(|id| {
        id.map(|id| iced::window::change_mode(id, iced::window::Mode::Hidden))
            .unwrap_or_else(Task::none)
    })
}

fn minimize_window() -> Task<Message> {
    iced::window::get_latest().then(|id| {
        id.map(|id| iced::window::minimize(id, true))
            .unwrap_or_else(Task::none)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowAction {
    Show,
    Hide,
    Minimize,
}

/// Coalesce queued window requests before starting an Iced window task.
fn resolve_window_signals(
    initially_hidden: bool,
    tray_available: bool,
    signals: impl IntoIterator<Item = WindowSignal>,
) -> Option<WindowAction> {
    let mut hidden = initially_hidden;
    let mut action = None;
    for signal in signals {
        let next = match signal {
            WindowSignal::Activate => WindowAction::Show,
            WindowSignal::Hotkey { visible } if hidden || !visible => WindowAction::Show,
            WindowSignal::Hotkey { .. } if tray_available => WindowAction::Hide,
            WindowSignal::Hotkey { .. } => WindowAction::Minimize,
        };
        hidden = next != WindowAction::Show;
        action = Some(next);
    }
    action
}

#[cfg(test)]
mod window_signal_tests {
    use super::{resolve_window_signals, WindowAction, WindowSignal};

    #[test]
    fn hotkey_hides_visible_window_and_restores_hidden_or_minimized_window() {
        let hotkey = |visible| WindowSignal::Hotkey { visible };
        assert_eq!(
            resolve_window_signals(false, true, [hotkey(true)]),
            Some(WindowAction::Hide)
        );
        assert_eq!(
            resolve_window_signals(false, true, [hotkey(false)]),
            Some(WindowAction::Show)
        );
        assert_eq!(
            resolve_window_signals(true, true, [hotkey(false)]),
            Some(WindowAction::Show)
        );
        assert_eq!(
            resolve_window_signals(false, false, [hotkey(true)]),
            Some(WindowAction::Minimize)
        );
    }

    #[test]
    fn repeated_hotkey_and_second_launch_resolve_in_order() {
        let visible = WindowSignal::Hotkey { visible: true };
        assert_eq!(
            resolve_window_signals(false, true, [visible, visible]),
            Some(WindowAction::Show)
        );
        assert_eq!(
            resolve_window_signals(false, true, [visible, WindowSignal::Activate]),
            Some(WindowAction::Show)
        );
        assert_eq!(resolve_window_signals(false, true, []), None);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    History,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Text,
    Link,
    Code,
    File,
    Image,
}

impl Filter {
    fn label(self) -> &'static str {
        match self {
            Filter::All => "全部",
            Filter::Text => "文本",
            Filter::Link => "链接",
            Filter::Code => "代码",
            Filter::File => "文件",
            Filter::Image => "图片",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Filter::All => Icon::List,
            Filter::Text => Icon::Text,
            Filter::Link => Icon::Link,
            Filter::Code => Icon::Code,
            Filter::File => Icon::File,
            Filter::Image => Icon::Image,
        }
    }

    fn matches(self, kind: ContentKind) -> bool {
        match self {
            Filter::All => true,
            Filter::Text => kind == ContentKind::Text,
            Filter::Link => kind == ContentKind::Link,
            Filter::Code => kind == ContentKind::Code,
            Filter::File => kind == ContentKind::File,
            Filter::Image => kind == ContentKind::Image,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Newest,
    Oldest,
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Newest => "最新优先",
            Self::Oldest => "最旧优先",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListDensity {
    #[default]
    Detailed,
    Compact,
}

#[derive(Debug, Clone, Copy)]
struct CardMenu {
    sequence: u32,
    position: iced::Point,
}

pub struct App {
    shared: SharedUi,
    worker: WorkerHandle,
    tray_rx: Option<std::sync::mpsc::Receiver<TrayCmd>>,
    activation_rx: std::sync::mpsc::Receiver<WindowSignal>,
    window_hidden: bool,
    window_id: Option<iced::window::Id>,
    hotkey_recording: bool,
    hotkey_record_error: Option<String>,
    state: UiState,
    tracking_control: TrackingControl,
    search: String,
    page: Page,
    filter: Filter,
    pinned_only: bool,
    sort_order: SortOrder,
    list_density: ListDensity,
    exclude_input: String,
    detail_menu_open: bool,
    card_menu: Option<CardMenu>,
    cursor_position: iced::Point,
    window_size: iced::Size,
    content_modal_for: Option<u32>,
    clear_history_confirmation_open: bool,
    copied_tick: u32,
    show_qrcode: bool,
    source_paths: HashMap<String, String>,
    chrome_sync_pending: bool,
    chrome_sync_attempts: u8,
    updates: UpdateCheck,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    CursorMoved(iced::Point),
    SearchChanged(String),
    Select(u32),
    OpenCardMenu(u32),
    CloseCardMenu,
    RecopySelected,
    Copy(u32),
    CopyPlainText(u32),
    Delete(Vec<u32>),
    ExcludeSource(String),
    Unexclude(String),
    ExcludeInput(String),
    ExcludeSubmit,
    SetRetention(Retention),
    ToggleTracking,
    RequestClearHistory,
    ConfirmClearHistory,
    CancelClearHistory,
    GoPage(Page),
    ToggleRaw,
    ToggleDetailMenu,
    SetPinned { sequences: Vec<u32>, pinned: bool },
    OpenContentModal(u32),
    CloseContentModal,
    OpenSource(String),
    OpenLink(String),
    WebSearch(String),
    ToggleQrCode,
    SetFilter(Filter),
    TogglePinnedOnly,
    SetSortOrder(SortOrder),
    ToggleListDensity,
    WindowDrag,
    WindowReady(iced::window::Id),
    WindowMinimize,
    WindowToggleMaximize,
    WindowResized(iced::Size),
    WindowRestored(iced::window::Id),
    WindowClose,
    SetHotkey(Hotkey),
    StartHotkeyRecord,
    CancelHotkeyRecord,
    RecordHotkeyKey(Key, Physical, Modifiers),
    FocusSearch,
    ClearSearch,
    CheckUpdate,
    DownloadUpdate,
    OpenReleases,
    OpenRepository,
}

impl App {
    pub fn boot(
        startup_position: Option<(f32, f32)>,
        window_size: iced::Size,
        activation_rx: std::sync::mpsc::Receiver<WindowSignal>,
        activation_tx: std::sync::mpsc::Sender<WindowSignal>,
    ) -> (Self, Task<Message>) {
        let shared: SharedUi = Default::default();
        let worker = spawn_worker(shared.clone(), activation_tx);
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
                activation_rx,
                window_hidden: false,
                window_id: None,
                hotkey_recording: false,
                hotkey_record_error: None,
                tracking_control: TrackingControl::default(),
                search: String::new(),
                page: Page::History,
                filter: Filter::All,
                pinned_only: false,
                sort_order: SortOrder::default(),
                list_density: ListDensity::default(),
                exclude_input: String::new(),
                detail_menu_open: false,
                card_menu: None,
                cursor_position: iced::Point::ORIGIN,
                window_size,
                content_modal_for: None,
                clear_history_confirmation_open: false,
                copied_tick: 0,
                show_qrcode: false,
                source_paths: HashMap::new(),
                chrome_sync_pending: true,
                chrome_sync_attempts: 0,
                updates: UpdateCheck::default(),
            },
            show_initial_window(startup_position),
        )
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let ticks = iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::Tick);
        let keys = if self.hotkey_recording {
            iced::event::listen_with(record_hotkey_event)
        } else {
            iced::keyboard::on_key_press(handle_key)
        };
        let window_resized =
            iced::window::resize_events().map(|(_, size)| Message::WindowResized(size));
        let cursor = iced::event::listen_with(cursor_event);
        Subscription::batch([ticks, keys, window_resized, cursor])
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let card_menu_was_open = self.card_menu.is_some();
        if !matches!(
            &message,
            Message::Tick | Message::CursorMoved(_) | Message::OpenCardMenu(_)
        ) {
            self.card_menu = None;
        }
        match message {
            Message::CursorMoved(position) => {
                self.cursor_position = position;
                Task::none()
            }
            Message::Tick => {
                self.updates.poll();
                let mut tasks = Vec::new();
                if self.chrome_sync_pending {
                    #[cfg(windows)]
                    {
                        if veya_windows::platform::chrome::apply_rounded_corners("Veya") {
                            self.chrome_sync_pending = false;
                        }
                    }
                    #[cfg(not(windows))]
                    {
                        self.chrome_sync_pending = false;
                    }
                    self.chrome_sync_attempts = self.chrome_sync_attempts.saturating_add(1);
                    if self.chrome_sync_attempts >= 4 {
                        self.chrome_sync_pending = false;
                    }
                }
                self.copied_tick = self.copied_tick.saturating_sub(1);
                let mut tray_cmds = Vec::new();
                let mut tray_disconnected = false;
                if let Some(rx) = &self.tray_rx {
                    loop {
                        match rx.try_recv() {
                            Ok(cmd) => tray_cmds.push(cmd),
                            Err(std::sync::mpsc::TryRecvError::Empty) => break,
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                tray_disconnected = true;
                                break;
                            }
                        }
                    }
                }
                let mut window_signals = Vec::new();
                for cmd in tray_cmds {
                    match cmd {
                        TrayCmd::Exit => return self.quit(),
                        TrayCmd::OpenWindow => window_signals.push(WindowSignal::Activate),
                        TrayCmd::OpenSettings => {
                            self.page = Page::Settings;
                            window_signals.push(WindowSignal::Activate);
                        }
                        other => {
                            tray::apply_tray_action(&other, &self.worker);
                        }
                    }
                }
                if tray_disconnected {
                    self.tray_rx = None;
                    if self.window_hidden {
                        window_signals.push(WindowSignal::Activate);
                    }
                }
                while let Ok(signal) = self.activation_rx.try_recv() {
                    window_signals.push(signal);
                }
                if let Some(action) = resolve_window_signals(
                    self.window_hidden,
                    self.tray_rx.is_some(),
                    window_signals,
                ) {
                    tasks.push(self.apply_window_action(action));
                }
                if let Ok(guard) = self.shared.lock() {
                    let mut next = guard.clone();
                    self.tracking_control.reconcile(&mut next);
                    next.selected = self
                        .state
                        .selected
                        .filter(|seq| next.cards.iter().any(|c| c.sequence == *seq))
                        .or_else(|| next.cards.first().map(|c| c.sequence));
                    next.expanded_raw = self.state.expanded_raw;
                    self.state = next;
                }
                self.ensure_visible_selection();
                self.cache_selected_source_path();
                if self
                    .content_modal_for
                    .is_some_and(|seq| !self.state.cards.iter().any(|card| card.sequence == seq))
                {
                    self.content_modal_for = None;
                }
                if self.card_menu.is_some_and(|menu| {
                    !self
                        .filtered_cards()
                        .iter()
                        .any(|card| card.sequence == menu.sequence)
                }) {
                    self.card_menu = None;
                }
                Task::batch(tasks)
            }
            Message::SearchChanged(q) => {
                self.search = q;
                self.ensure_visible_selection();
                Task::none()
            }
            Message::Select(seq) => {
                self.select_history_card(seq);
                Task::none()
            }
            Message::OpenCardMenu(seq) => {
                if self.state.cards.iter().any(|card| card.sequence == seq) {
                    self.select_history_card(seq);
                    self.card_menu = Some(CardMenu {
                        sequence: seq,
                        position: self.cursor_position,
                    });
                }
                Task::none()
            }
            Message::CloseCardMenu => Task::none(),
            Message::RecopySelected | Message::Copy(_) => {
                let seq = match message {
                    Message::Copy(s) => Some(s),
                    _ => self.state.selected,
                };
                if let Some(seq) = seq {
                    if self.state.cards.iter().any(|card| card.sequence == seq) {
                        let _ = self.worker.cmd_tx.send(WorkerCmd::Recopy { sequence: seq });
                        self.copied_tick = 6; // ~1.5s visual feedback
                    }
                }
                Task::none()
            }
            Message::CopyPlainText(seq) => {
                if let Some(card) = self.state.cards.iter().find(|c| c.sequence == seq) {
                    let text = card.full_content.clone();
                    let _ = self.worker.cmd_tx.send(WorkerCmd::RecopyText { text });
                    self.copied_tick = 6;
                }
                Task::none()
            }
            Message::Delete(sequences) => {
                let _ = self
                    .worker
                    .cmd_tx
                    .send(WorkerCmd::DeleteRecord { sequences });
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
                self.tracking_control
                    .request_toggle(&self.worker.cmd_tx, &mut self.state);
                Task::none()
            }
            Message::RequestClearHistory => {
                self.clear_history_confirmation_open = true;
                Task::none()
            }
            Message::ConfirmClearHistory => {
                let _ = self.worker.cmd_tx.send(WorkerCmd::ClearHistory);
                self.clear_history_confirmation_open = false;
                Task::none()
            }
            Message::CancelClearHistory => {
                self.clear_history_confirmation_open = false;
                Task::none()
            }
            Message::GoPage(p) => {
                self.cancel_hotkey_recording();
                self.page = p;
                self.content_modal_for = None;
                Task::none()
            }
            Message::ToggleRaw => {
                let next = !self.state.expanded_raw;
                self.state.expanded_raw = next;
                let _ = self.worker.cmd_tx.send(WorkerCmd::SetExpandedRaw(next));
                Task::none()
            }
            Message::ToggleDetailMenu => {
                self.detail_menu_open = !self.detail_menu_open;
                Task::none()
            }
            Message::SetPinned { sequences, pinned } => {
                let _ = self
                    .worker
                    .cmd_tx
                    .send(WorkerCmd::SetPinned { sequences, pinned });
                self.detail_menu_open = false;
                Task::none()
            }
            Message::OpenContentModal(seq) => {
                if self.state.cards.iter().any(|card| card.sequence == seq) {
                    self.content_modal_for = Some(seq);
                }
                Task::none()
            }
            Message::CloseContentModal => {
                self.content_modal_for = None;
                Task::none()
            }
            Message::OpenSource(app) => {
                veya_windows::platform::shell::open_source(&app);
                Task::none()
            }
            Message::OpenLink(url) => {
                veya_windows::platform::shell::open_link(&url);
                Task::none()
            }
            Message::CheckUpdate => {
                self.updates.start();
                Task::none()
            }
            Message::DownloadUpdate => {
                self.updates.download_and_install();
                Task::none()
            }
            Message::OpenReleases => {
                veya_windows::platform::shell::open_link(
                    veya_windows::platform::update::LATEST_RELEASE_URL,
                );
                Task::none()
            }
            Message::OpenRepository => {
                veya_windows::platform::shell::open_link(
                    veya_windows::platform::update::REPOSITORY_URL,
                );
                Task::none()
            }
            Message::WebSearch(query) => {
                veya_windows::platform::shell::web_search(&query);
                Task::none()
            }
            Message::ToggleQrCode => {
                self.show_qrcode = !self.show_qrcode;
                Task::none()
            }
            Message::SetFilter(f) => {
                self.filter = f;
                self.ensure_visible_selection();
                Task::none()
            }
            Message::TogglePinnedOnly => {
                self.pinned_only = !self.pinned_only;
                self.ensure_visible_selection();
                Task::none()
            }
            Message::SetSortOrder(order) => {
                self.sort_order = order;
                self.ensure_visible_selection();
                scrollable::snap_to(history_scroll_id(), scrollable::RelativeOffset::START)
            }
            Message::ToggleListDensity => {
                self.list_density = match self.list_density {
                    ListDensity::Detailed => ListDensity::Compact,
                    ListDensity::Compact => ListDensity::Detailed,
                };
                Task::none()
            }
            Message::WindowDrag => self
                .window_id
                .map(iced::window::drag)
                .unwrap_or_else(Task::none),
            Message::WindowReady(id) => {
                self.window_id = Some(id);
                Task::none()
            }
            Message::WindowMinimize => {
                self.cancel_hotkey_recording();
                self.window_hidden = false;
                minimize_window()
            }
            Message::WindowToggleMaximize => iced::window::get_latest().then(|id| {
                id.map(iced::window::toggle_maximize)
                    .unwrap_or_else(Task::none)
            }),
            Message::WindowResized(size) => {
                self.window_size = size;
                self.chrome_sync_pending = true;
                self.chrome_sync_attempts = 0;
                Task::none()
            }
            Message::WindowRestored(id) => {
                self.window_id = Some(id);
                #[cfg(windows)]
                veya_windows::platform::singleton::ensure_main_window_visible();
                self.chrome_sync_pending = true;
                iced::window::gain_focus(id)
            }
            Message::WindowClose => {
                self.cancel_hotkey_recording();
                if self.tray_rx.is_some() {
                    self.window_hidden = true;
                    hide_window()
                } else {
                    self.quit()
                }
            }
            Message::SetHotkey(selection) => {
                self.hotkey_recording = false;
                self.hotkey_record_error = None;
                if self
                    .worker
                    .cmd_tx
                    .send(WorkerCmd::SetHotkey(selection))
                    .is_err()
                {
                    self.hotkey_record_error = Some("快捷键服务已停止，请重启 Veya".into());
                    #[cfg(windows)]
                    veya_windows::platform::request_hotkey_recording(false);
                }
                Task::none()
            }
            Message::StartHotkeyRecord => {
                #[cfg(windows)]
                {
                    if !veya_windows::platform::request_hotkey_recording(true) {
                        self.hotkey_record_error = Some("快捷键服务尚未就绪，请稍后重试".into());
                        return Task::none();
                    }
                }
                self.hotkey_recording = true;
                self.hotkey_record_error = None;
                Task::none()
            }
            Message::CancelHotkeyRecord => {
                self.cancel_hotkey_recording();
                Task::none()
            }
            Message::RecordHotkeyKey(key, physical, modifiers) => {
                if !self.hotkey_recording {
                    return Task::none();
                }
                if matches!(key, Key::Named(Named::Escape)) {
                    self.cancel_hotkey_recording();
                    return Task::none();
                }
                if matches!(
                    key,
                    Key::Named(Named::Alt | Named::Control | Named::Shift | Named::Super)
                ) {
                    return Task::none();
                }
                let mut flags = 0;
                if modifiers.alt() {
                    flags |= MOD_ALT;
                }
                if modifiers.control() {
                    flags |= MOD_CONTROL;
                }
                if modifiers.shift() {
                    flags |= MOD_SHIFT;
                }
                let shortcut = recorded_key(&key, physical).and_then(|vk| Hotkey::new(flags, vk));
                if let Some(shortcut) = shortcut.filter(|_| !modifiers.logo()) {
                    self.hotkey_recording = false;
                    self.hotkey_record_error = None;
                    if self
                        .worker
                        .cmd_tx
                        .send(WorkerCmd::SetHotkey(shortcut))
                        .is_err()
                    {
                        self.hotkey_record_error = Some("快捷键服务已停止，请重启 Veya".into());
                        #[cfg(windows)]
                        veya_windows::platform::request_hotkey_recording(false);
                    }
                } else {
                    self.hotkey_record_error =
                        Some("请按 Ctrl 或 Alt + 字母、数字、Space 或 F1–F11；Esc 取消".into());
                }
                Task::none()
            }
            Message::FocusSearch => {
                if self.content_modal_for.is_some() || self.clear_history_confirmation_open {
                    return Task::none();
                }
                self.page = Page::History;
                text_input::focus(search_input_id())
            }
            Message::ClearSearch => {
                if self.clear_history_confirmation_open {
                    self.clear_history_confirmation_open = false;
                    return Task::none();
                }
                if self.content_modal_for.is_some() {
                    self.content_modal_for = None;
                    return Task::none();
                }
                if card_menu_was_open {
                    return Task::none();
                }
                if !self.search.is_empty() {
                    self.search.clear();
                    self.ensure_visible_selection();
                    Task::none()
                } else {
                    self.page = Page::History;
                    Task::none()
                }
            }
        }
    }

    fn select_history_card(&mut self, seq: u32) {
        self.state.selected = Some(seq);
        self.detail_menu_open = false;
        self.content_modal_for = None;
        self.show_qrcode = false;
        if let Ok(mut guard) = self.shared.lock() {
            guard.selected = Some(seq);
        }
        self.cache_selected_source_path();
    }

    fn apply_window_action(&mut self, action: WindowAction) -> Task<Message> {
        match action {
            WindowAction::Show => {
                self.window_hidden = false;
                activate_window()
            }
            WindowAction::Hide => {
                self.cancel_hotkey_recording();
                self.window_hidden = true;
                hide_window()
            }
            WindowAction::Minimize => {
                self.cancel_hotkey_recording();
                self.window_hidden = false;
                minimize_window()
            }
        }
    }

    fn quit(&self) -> Task<Message> {
        let _ = self.worker.cmd_tx.send(WorkerCmd::SetTracking {
            on: false,
            request_id: None,
        });
        #[cfg(windows)]
        veya_windows::platform::request_shutdown();
        iced::window::get_latest().then(|id| id.map(iced::window::close).unwrap_or_else(Task::none))
    }

    fn cancel_hotkey_recording(&mut self) {
        if self.hotkey_recording {
            self.hotkey_recording = false;
            #[cfg(windows)]
            veya_windows::platform::request_hotkey_recording(false);
        }
        self.hotkey_record_error = None;
    }

    fn cache_selected_source_path(&mut self) {
        let Some(card) = self
            .state
            .selected
            .and_then(|seq| self.state.cards.iter().find(|card| card.sequence == seq))
        else {
            return;
        };
        let app = card.source_app.clone();
        if self.source_paths.contains_key(&app) {
            return;
        }

        #[cfg(windows)]
        let display_path = veya_windows::platform::icon::resolve_exe_path(&app)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| app.clone());
        #[cfg(not(windows))]
        let display_path = app.clone();

        self.source_paths.insert(app, display_path);
    }

    pub(super) fn source_path_for(&self, app: &str) -> String {
        self.source_paths
            .get(app)
            .cloned()
            .unwrap_or_else(|| app.to_string())
    }

    pub fn view(&self) -> Element<'_, Message> {
        let base: Element<'_, Message> = mouse_area(
            container(
                row![
                    container(self.sidebar())
                        .width(Length::Fixed(192.0))
                        .height(Length::Fill),
                    theme::vdivider(),
                    column![self.top_bar(), self.main_body()]
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .spacing(0),
                ]
                .spacing(0)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(1)
            .style(theme::shell_style),
        )
        .on_press(Message::WindowDrag)
        .into();

        let context_layer: Element<'_, Message> = self
            .card_menu
            .and_then(|menu| {
                self.state
                    .cards
                    .iter()
                    .find(|card| card.sequence == menu.sequence)
                    .map(|card| self.card_context_menu(card, menu.position))
            })
            .unwrap_or_else(|| Space::new(Length::Fill, Length::Fill).into());
        let base: Element<'_, Message> = stack([base, context_layer]).into();

        if self.clear_history_confirmation_open {
            stack([base, self.clear_history_confirmation_modal()]).into()
        } else if let Some(card) = self
            .content_modal_for
            .and_then(|seq| self.state.cards.iter().find(|card| card.sequence == seq))
        {
            stack([base, self.content_modal(card)]).into()
        } else {
            base
        }
    }

    fn sidebar(&self) -> Element<'_, Message> {
        const LOGO: &[u8] = include_bytes!("../../icons/64x64.png");
        let logo = row![
            container(
                iced::widget::image(iced::widget::image::Handle::from_bytes(LOGO))
                    .width(30)
                    .height(30),
            )
            .padding(0),
            column![
                text("Veya")
                    .size(19)
                    .color(theme::INK)
                    .font(crate::font::name_font()),
                meta("你的剪贴板，有记忆。").size(11).color(theme::MUTED),
            ]
            .spacing(1),
        ]
        .spacing(11)
        .align_y(Alignment::Center);

        let nav_history = button(
            row![
                icons::icon(
                    Icon::History,
                    if self.page == Page::History {
                        theme::INK
                    } else {
                        theme::MUTED
                    },
                    18.0
                ),
                body("历史记录")
                    .size(14)
                    .color(if self.page == Page::History {
                        theme::INK
                    } else {
                        theme::MUTED
                    })
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .on_press(Message::GoPage(Page::History))
        .style(theme::nav_button(self.page == Page::History))
        .padding(pad(11.0, 14.0, 11.0, 14.0))
        .width(Length::Fill);

        let nav_settings = button(
            row![
                icons::icon(
                    Icon::Settings,
                    if self.page == Page::Settings {
                        theme::INK
                    } else {
                        theme::MUTED
                    },
                    18.0
                ),
                body("设置").size(14).color(if self.page == Page::Settings {
                    theme::INK
                } else {
                    theme::MUTED
                })
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .on_press(Message::GoPage(Page::Settings))
        .style(theme::nav_button(self.page == Page::Settings))
        .padding(pad(11.0, 14.0, 11.0, 14.0))
        .width(Length::Fill);

        let dot_color = if self.state.tracking {
            theme::OK
        } else {
            theme::WARN
        };
        let status_text = if self.state.tracking {
            "正在追踪"
        } else {
            "已暂停"
        };
        let mut footer = column![
            row![
                container(Space::new(7.0, 7.0))
                    .width(Length::Fixed(7.0))
                    .height(Length::Fixed(7.0))
                    .style(move |_t| iced::widget::container::Style {
                        background: Some(theme::bg(dot_color)),
                        border: iced::Border {
                            color: Color::from_rgba(0.18, 0.82, 0.58, 0.5),
                            width: 1.0,
                            radius: 3.5.into(),
                        },
                        ..Default::default()
                    }),
                meta(status_text).size(12).color(theme::MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            meta(format!(
                "{} 条记录 · {}",
                self.state.record_count,
                self.state.retention.label()
            ))
            .size(11)
            .color(theme::FAINT),
        ]
        .spacing(5);
        if !self.state.status_note.is_empty() {
            footer = footer.push(
                meta(self.state.status_note.clone())
                    .size(11)
                    .color(theme::WARN),
            );
        }

        container(
            column![
                mouse_area(container(logo).padding(pad(20.0, 14.0, 22.0, 14.0)))
                    .on_press(Message::WindowDrag),
                column![nav_history, nav_settings]
                    .spacing(5)
                    .padding(pad(0.0, 10.0, 0.0, 10.0)),
                Space::with_height(Length::Fill),
                container(footer).padding(pad(16.0, 16.0, 18.0, 16.0)),
            ]
            .spacing(0),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::sidebar_style)
        .into()
    }

    fn top_bar(&self) -> Element<'_, Message> {
        let left_part: Element<'_, Message> = match self.page {
            Page::History => {
                let search_input = text_input("搜索内容、来源或去向…", &self.search)
                    .id(search_input_id())
                    .padding(pad(8.0, 8.0, 8.0, 4.0))
                    .size(13)
                    .style(theme::input_style)
                    .width(Length::Fill);
                let search_input = if self.content_modal_for.is_some() {
                    search_input
                } else {
                    search_input.on_input(Message::SearchChanged)
                };

                container(
                    row![
                        icons::icon(Icon::Search, theme::MUTED, 16.0),
                        search_input,
                        theme::keycap("Ctrl"),
                        theme::keycap("K"),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .padding(pad(5.0, 10.0, 5.0, 12.0))
                .width(Length::Fill)
                .style(theme::search_shell_style)
                .into()
            }
            Page::Settings => container(
                row![
                    icons::icon(Icon::Settings, theme::ACCENT, 18.0),
                    text("系统设置")
                        .size(15)
                        .color(theme::INK)
                        .font(crate::font::name_font()),
                    meta("· 隐私控制与本地存储管理")
                        .size(12)
                        .color(theme::FAINT),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(pad(6.0, 4.0, 6.0, 4.0))
            .width(Length::Fill)
            .into(),
        };

        let pause_icon = if self.state.tracking {
            Icon::Pause
        } else {
            Icon::Play
        };
        let pause_label = if self.state.tracking {
            "暂停"
        } else {
            "恢复"
        };
        let pause = button(
            row![
                icons::icon(pause_icon, theme::INK, 14.0),
                body(pause_label).size(13)
            ]
            .spacing(7)
            .align_y(Alignment::Center),
        )
        .on_press(Message::ToggleTracking)
        .style(theme::elevated_button)
        .padding(pad(8.0, 14.0, 8.0, 14.0))
        .width(Length::Fixed(98.0));

        let win = row![
            button(icons::icon(Icon::Minimize, theme::MUTED, 12.0))
                .on_press(Message::WindowMinimize)
                .style(theme::win_button)
                .padding(pad(6.0, 8.0, 6.0, 8.0)),
            button(icons::icon(Icon::Maximize, theme::MUTED, 12.0))
                .on_press(Message::WindowToggleMaximize)
                .style(theme::win_button)
                .padding(pad(6.0, 8.0, 6.0, 8.0)),
            button(icons::icon(Icon::Close, theme::MUTED, 12.0))
                .on_press(Message::WindowClose)
                .style(theme::win_button)
                .padding(pad(6.0, 8.0, 6.0, 8.0)),
        ]
        .spacing(2);

        let bar = row![
            container(left_part).width(Length::FillPortion(7)),
            mouse_area(
                container(meta("拖动窗口").size(11).color(theme::FAINT))
                    .width(Length::Fixed(96.0))
                    .height(Length::Fixed(42.0))
                    .center_x(Length::Fixed(96.0))
                    .center_y(Length::Fixed(42.0)),
            )
            .on_press(Message::WindowDrag),
            pause,
            Space::with_width(6.0),
            win,
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        container(bar)
            .padding(pad(12.0, 16.0, 12.0, 16.0))
            .width(Length::Fill)
            .style(theme::topbar_style)
            .into()
    }

    fn main_body(&self) -> Element<'_, Message> {
        match self.page {
            Page::History => column![
                theme::hdivider(),
                self.history_toolbar(),
                theme::hdivider(),
                row![
                    container(self.history_column())
                        .width(Length::Fixed(320.0))
                        .height(Length::Fill),
                    container(self.detail_panel())
                        .width(Length::Fill)
                        .height(Length::Fill),
                ]
                .spacing(12)
                .padding(pad(0.0, 8.0, 8.0, 8.0))
                .height(Length::Fill)
            ]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            Page::Settings => column![
                theme::hdivider(),
                container(self.settings_panel())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(24)
            ]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        }
    }
}

/// One usage-timeline row: time | vertical spine + glowing dot | app avatar + name + Ctrl+V badge + window/doc.
fn timeline_row<'a, Message>(
    time: &'a str,
    app: &'a str,
    method: &'a str,
    window: &'a str,
    _first: bool,
    last: bool,
) -> Element<'a, Message>
where
    Message: 'a,
{
    let spine = if last {
        column![
            theme::spine_dot::<Message>(true),
            theme::spine_line::<Message>(14.0),
        ]
        .align_x(Alignment::Center)
        .width(Length::Fixed(14.0))
    } else {
        column![
            theme::spine_dot::<Message>(true),
            theme::spine_line::<Message>(30.0),
        ]
        .align_x(Alignment::Center)
        .width(Length::Fixed(14.0))
    };

    let mut info = column![row![
        icons::app_avatar::<Message>(app, 18.0),
        body(short_app(app)).size(13).font(crate::font::name_font()),
        container(text(method).size(10).color(theme::MUTED))
            .padding(pad(1.0, 5.0, 1.0, 5.0))
            .style(theme::chip_container),
    ]
    .spacing(6)
    .align_y(Alignment::Center),]
    .spacing(2)
    .width(Length::Fill);

    let win = window.trim();
    if !win.is_empty() && win != "-" && win != "unknown" {
        let tag = if win.contains('\\') || win.contains('/') {
            format!("📄 {win}")
        } else {
            win.to_string()
        };
        info = info.push(meta(tag).size(11).color(theme::FAINT));
    }

    row![
        text(time).size(11).color(theme::FAINT),
        Space::with_width(6.0),
        spine,
        Space::with_width(6.0),
        info,
    ]
    .align_y(Alignment::Start)
    .into()
}

/// Helper to generate a handsome context preview card (web page or window title)
fn extract_context_preview(content: &str, window: &str) -> Option<(Icon, String, String)> {
    let t = content.trim();
    if t.starts_with("http://") || t.starts_with("https://") {
        if let Some(host) = t.split("://").nth(1) {
            let host_part = host.split('/').next().unwrap_or(host);
            let path_part = host.split_once('/').map(|(_, p)| p).unwrap_or("");
            let title = if host_part.contains("chatgpt.com") {
                "ChatGPT 会话".to_string()
            } else if host_part.contains("github.com") {
                let repo = path_part.trim_matches('/');
                if !repo.is_empty() {
                    format!("GitHub - {repo}")
                } else {
                    "GitHub 仓库".to_string()
                }
            } else if host_part.contains("linux.do") {
                "LINUX DO 社区".to_string()
            } else if host_part.contains("bilibili.com") {
                "哔哩哔哩".to_string()
            } else if !window.is_empty() && window != "-" && window != "unknown" {
                window.to_string()
            } else {
                format!("网页链接")
            };
            return Some((Icon::Globe, title, host_part.to_string()));
        }
    }
    let win = window.trim();
    if !win.is_empty() && win != "-" && win != "unknown" {
        return Some((Icon::External, win.to_string(), "窗口上下文".to_string()));
    }
    None
}

fn short_app(exe: &str) -> String {
    let name = base_exe(exe);
    match name.to_ascii_lowercase().as_str() {
        "chrome.exe" | "chrome" => "Chrome".into(),
        "idea64.exe" | "idea" => "IntelliJ IDEA".into(),
        "datagrip.exe" | "datagrip" => "DataGrip".into(),
        "wechat.exe" | "weixin.exe" | "wechat" | "weixin" => "微信".into(),
        "windowsterminal.exe" | "terminal" => "Windows Terminal".into(),
        "explorer.exe" | "explorer" => "Explorer".into(),
        "snipaste.exe" | "snipaste" => "Snipaste".into(),
        "msedge.exe" | "edge" => "Microsoft Edge".into(),
        "code.exe" | "vscode" => "VS Code".into(),
        _ => name
            .trim_end_matches(".exe")
            .trim_end_matches(".EXE")
            .to_string(),
    }
}

/// Semantic title in Header: concise, clean, never breaks mid-word
fn detail_title(card: &CardView) -> String {
    match &card.payload {
        CardPayloadView::Files(paths) => return format!("文件列表 · {} 项", paths.len()),
        CardPayloadView::Image { .. } => return "剪贴板图片".to_string(),
        CardPayloadView::Text => {}
    }

    let t = card.full_content.trim();
    if card.kind == ContentKind::Link {
        if let Some(host) = t.split("://").nth(1) {
            let host_clean = host.split('/').next().unwrap_or(host);
            let path_part = host.split_once('/').map(|(_, p)| p).unwrap_or("");
            if host_clean.contains("chatgpt.com") {
                return "ChatGPT 对话链接".to_string();
            } else if host_clean.contains("github.com") {
                let repo = path_part.trim_matches('/');
                if !repo.is_empty() {
                    return format!("GitHub - {repo}");
                }
                return "GitHub 仓库".to_string();
            } else if host_clean.contains("linux.do") {
                return "LINUX DO 话题链接".to_string();
            } else if host_clean.contains("bilibili.com") {
                return "哔哩哔哩视频".to_string();
            } else if host_clean.contains("zhihu.com") {
                return "知乎内容".to_string();
            } else if host_clean.contains("google.com") {
                return "Google 网页链接".to_string();
            }
            return format!("网页链接 · {host_clean}");
        }
        return "网页链接".to_string();
    } else if card.kind == ContentKind::Code {
        if t.starts_with("SELECT ")
            || t.starts_with("INSERT ")
            || t.starts_with("UPDATE ")
            || t.starts_with("DELETE ")
        {
            return "SQL 查询语句".to_string();
        }
        let first_line = t.lines().next().unwrap_or("").trim();
        if !first_line.is_empty() && first_line.chars().count() <= 28 {
            return format!("代码 · {first_line}");
        }
        return "代码片段".to_string();
    } else {
        let first_line = t.lines().next().unwrap_or("").trim();
        if first_line.chars().count() <= 24 && !t.contains('\n') {
            return first_line.to_string();
        }
        let preview = card.content_preview.trim();
        if preview.chars().count() > 24 {
            let s: String = preview.chars().take(24).collect();
            format!("{s}…")
        } else {
            preview.to_string()
        }
    }
}

/// Detail subtitle with domain hint when applicable
fn detail_subtitle(card: &CardView) -> String {
    match &card.payload {
        CardPayloadView::Files(paths) => {
            return format!(
                "{} 项 · {} · {}",
                paths.len(),
                card.relative_time,
                card.time_full
            );
        }
        CardPayloadView::Image { width, height, .. } => {
            return format!("{width} × {height} px · {}", card.time_full);
        }
        CardPayloadView::Text => {}
    }

    let t = card.full_content.trim();
    if card.kind == ContentKind::Link {
        if let Some(host) = t.split("://").nth(1) {
            let host_clean = host.split('/').next().unwrap_or(host);
            return format!(
                "{} · {} · {}",
                host_clean, card.relative_time, card.time_full
            );
        }
    }
    format!("{} · {}", card.relative_time, card.time_full)
}

/// Truncate file path smoothly (e.g. C:\Program Files\...\chrome.exe) without breaking words
fn truncate_path(path: &str, max_chars: usize) -> String {
    if path.chars().count() <= max_chars {
        return path.to_string();
    }
    let parts: Vec<&str> = path.split('\\').collect();
    if parts.len() >= 3 {
        let first = parts[0];
        let second = parts[1];
        let last = parts.last().unwrap();
        let short = format!("{first}\\{second}\\...\\{last}");
        if short.chars().count() <= max_chars {
            return short;
        }
    }
    let s: String = path.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{s}…")
}

fn base_exe(name: &str) -> &str {
    name.split(" (").next().unwrap_or(name)
}

fn handle_key(key: Key, mods: Modifiers) -> Option<Message> {
    if mods.command() && matches!(&key, Key::Character(c) if c.as_str().eq_ignore_ascii_case("k")) {
        return Some(Message::FocusSearch);
    }
    match key {
        Key::Named(Named::Enter) => Some(Message::RecopySelected),
        Key::Named(Named::Escape) => Some(Message::ClearSearch),
        _ => None,
    }
}

fn cursor_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::CursorMoved(position))
        }
        _ => None,
    }
}

/// Recording observes key presses even if another widget had focus before the
/// user clicked "更改". The exclusion input is removed during recording too.
fn record_hotkey_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            ..
        }) => Some(Message::RecordHotkeyKey(key, physical_key, modifiers)),
        iced::Event::Window(iced::window::Event::Unfocused) => Some(Message::CancelHotkeyRecord),
        _ => None,
    }
}

fn recorded_key(key: &Key, physical: Physical) -> Option<u32> {
    let logical = match key {
        Key::Named(Named::Space) => Some(0x20),
        Key::Named(Named::F1) => Some(0x70),
        Key::Named(Named::F2) => Some(0x71),
        Key::Named(Named::F3) => Some(0x72),
        Key::Named(Named::F4) => Some(0x73),
        Key::Named(Named::F5) => Some(0x74),
        Key::Named(Named::F6) => Some(0x75),
        Key::Named(Named::F7) => Some(0x76),
        Key::Named(Named::F8) => Some(0x77),
        Key::Named(Named::F9) => Some(0x78),
        Key::Named(Named::F10) => Some(0x79),
        Key::Named(Named::F11) => Some(0x7A),
        Key::Character(text) => {
            let mut chars = text.chars();
            let ch = chars.next()?;
            if chars.next().is_none() && ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_uppercase() as u32)
            } else {
                None
            }
        }
        _ => None,
    };
    logical.or_else(|| match physical {
        Physical::Code(Code::Digit0) => Some(b'0' as u32),
        Physical::Code(Code::Digit1) => Some(b'1' as u32),
        Physical::Code(Code::Digit2) => Some(b'2' as u32),
        Physical::Code(Code::Digit3) => Some(b'3' as u32),
        Physical::Code(Code::Digit4) => Some(b'4' as u32),
        Physical::Code(Code::Digit5) => Some(b'5' as u32),
        Physical::Code(Code::Digit6) => Some(b'6' as u32),
        Physical::Code(Code::Digit7) => Some(b'7' as u32),
        Physical::Code(Code::Digit8) => Some(b'8' as u32),
        Physical::Code(Code::Digit9) => Some(b'9' as u32),
        _ => None,
    })
}

fn detail_content_preview(content: &str) -> (String, bool) {
    const MAX_LINES: usize = 2;
    const MAX_CELLS_PER_LINE: usize = 56;

    let mut lines = content.lines();
    let mut preview = String::new();
    let mut truncated = false;
    for line_index in 0..MAX_LINES {
        let Some(line) = lines.next() else { break };
        if line_index > 0 {
            preview.push('\n');
        }
        let mut width = 0;
        for ch in line.chars() {
            let cells = if ch.is_ascii() { 1 } else { 2 };
            if width + cells > MAX_CELLS_PER_LINE {
                truncated = true;
                break;
            }
            preview.push(if ch.is_control() { ' ' } else { ch });
            width += cells;
        }
        if truncated {
            break;
        }
    }
    truncated |= lines.next().is_some();
    if truncated {
        preview.push('…');
    }
    (preview, truncated)
}

fn visible_cards<'a>(
    all_cards: &'a [CardView],
    filter: Filter,
    pinned_only: bool,
    order: SortOrder,
    search: &str,
) -> Vec<&'a CardView> {
    let q = search.trim();
    let mut cards: Vec<_> = all_cards
        .iter()
        .filter(|card| {
            filter.matches(card.kind)
                && (!pinned_only || card.pinned)
                && veya_core::match_field(
                    &card.full_content,
                    &card.source_app,
                    card.used_in_apps.iter().map(|s| s.as_str()),
                    q,
                )
                .is_some()
        })
        .collect();
    cards.sort_by(|a, b| {
        let cmp = (a.first_ms, a.sequence).cmp(&(b.first_ms, b.sequence));
        match order {
            SortOrder::Newest => cmp.reverse(),
            SortOrder::Oldest => cmp,
        }
    });
    cards
}

fn history_scroll_id() -> scrollable::Id {
    scrollable::Id::new("history-list")
}

fn compact_preview(content: &str, max_cells: usize) -> String {
    let first_line = content.lines().next().unwrap_or_default();
    let mut result = String::new();
    let mut width = 0;
    for ch in first_line.chars() {
        let char_width = if ch.is_ascii() { 1 } else { 2 };
        if width + char_width > max_cells {
            result.push('…');
            return result;
        }
        result.push(ch);
        width += char_width;
    }
    if content.lines().nth(1).is_some() {
        result.push('…');
    }
    result
}

fn search_input_id() -> text_input::Id {
    text_input::Id::new("veya-search")
}

#[cfg(test)]
mod history_controls_tests {
    use super::*;

    fn card(sequence: u32, first_ms: i64, content: &str) -> CardView {
        CardView {
            sequence,
            raw_count: 1,
            raw_sequences: vec![sequence],
            content_preview: content.to_string(),
            full_content: content.to_string(),
            payload: CardPayloadView::Text,
            kind: format::content_kind(content),
            pinned: false,
            source_app: "test.exe".to_string(),
            source_confidence: veya_core::SourceConfidence::Exact,
            source_window: String::new(),
            first_ms,
            time_full: String::new(),
            relative_time: String::new(),
            time_range: String::new(),
            used_in: Vec::new(),
            used_in_apps: Vec::new(),
            has_paste_activity: false,
            paste_detail: "",
        }
    }

    #[test]
    fn filtering_search_and_sort_use_actual_content_and_stable_ties() {
        let mut cards = vec![
            card(3, 200, "SELECT * FROM users"),
            card(1, 100, "https://example.com"),
            card(2, 200, r"C:\Users\admin\report.pdf"),
            card(4, 200, "ordinary note"),
        ];
        cards[0].pinned = true;
        let sequences = |filter, pinned_only, sort, query| {
            visible_cards(&cards, filter, pinned_only, sort, query)
                .iter()
                .map(|card| card.sequence)
                .collect::<Vec<_>>()
        };
        assert_eq!(sequences(Filter::Code, false, SortOrder::Newest, ""), [3]);
        assert_eq!(
            format::content_kind("// Rust snippet\nfn process() {}"),
            ContentKind::Code
        );
        assert_eq!(sequences(Filter::Link, false, SortOrder::Newest, ""), [1]);
        assert_eq!(
            sequences(Filter::Text, false, SortOrder::Newest, ""),
            [4, 2]
        );
        assert_eq!(
            sequences(Filter::Text, false, SortOrder::Newest, "report"),
            [2]
        );
        assert_eq!(
            sequences(Filter::All, false, SortOrder::Oldest, ""),
            [1, 2, 3, 4]
        );
        assert_eq!(
            sequences(Filter::All, false, SortOrder::Newest, ""),
            [4, 3, 2, 1]
        );
        assert!(sequences(Filter::File, false, SortOrder::Newest, "").is_empty());
        assert!(sequences(Filter::Image, false, SortOrder::Newest, "").is_empty());
        assert_eq!(sequences(Filter::All, true, SortOrder::Newest, ""), [3]);
        assert_eq!(sequences(Filter::Code, true, SortOrder::Newest, ""), [3]);
    }

    #[test]
    fn file_and_image_filters_follow_the_captured_payload_kind() {
        let mut cards = vec![
            card(1, 100, r"C:\Users\admin\Pictures\photo.png"),
            card(2, 200, "image 640 × 480"),
        ];
        cards[0].payload =
            CardPayloadView::Files(vec![r"C:\Users\admin\Pictures\photo.png".into()]);
        cards[0].kind = ContentKind::File;
        cards[1].payload = CardPayloadView::Image {
            handle: iced::widget::image::Handle::from_bytes(vec![137, 80, 78, 71]),
            width: 640,
            height: 480,
            encoded_bytes: 4,
        };
        cards[1].kind = ContentKind::Image;

        let sequences = |filter| {
            visible_cards(&cards, filter, false, SortOrder::Newest, "")
                .iter()
                .map(|card| card.sequence)
                .collect::<Vec<_>>()
        };
        assert_eq!(sequences(Filter::File), [1]);
        assert_eq!(sequences(Filter::Image), [2]);
    }

    #[test]
    fn compact_preview_keeps_one_bounded_line() {
        assert_eq!(compact_preview("hello\nworld", 26), "hello…");
        assert_eq!(compact_preview("中文中文中文", 6), "中文中…");
    }

    #[test]
    fn detail_preview_shows_two_bounded_lines_and_marks_hidden_content() {
        assert_eq!(
            detail_content_preview("short text"),
            ("short text".into(), false)
        );
        assert_eq!(
            detail_content_preview("first\nsecond\nthird"),
            ("first\nsecond…".into(), true),
        );
        let (preview, truncated) = detail_content_preview(&"中".repeat(40));
        assert!(truncated);
        assert_eq!(preview, format!("{}…", "中".repeat(28)));
    }
}
