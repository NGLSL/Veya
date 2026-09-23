use super::*;
use iced::widget::column;

impl App {
    /// Modern settings panel with 660px max width constraint and compact card group rows
    pub(super) fn settings_panel(&self) -> Element<'_, Message> {
        let chip = |r: Retention, current: Retention| {
            button(text(r.label()).size(12))
                .on_press(Message::SetRetention(r))
                .style(theme::chip_button(r == current))
                .padding(pad(6.0, 12.0, 6.0, 12.0))
        };

        // Modern sliding toggle switch representation
        let toggle_switch = button(
            row![if self.state.tracking {
                row![
                    text("已开启").size(12).color(theme::OK),
                    container(Space::new(12.0, 12.0))
                        .width(Length::Fixed(12.0))
                        .height(Length::Fixed(12.0))
                        .style(|_t| iced::widget::container::Style {
                            background: Some(theme::bg(theme::OK)),
                            border: iced::Border {
                                color: theme::OK,
                                width: 1.0,
                                radius: 6.0.into(),
                            },
                            ..Default::default()
                        }),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            } else {
                row![
                    container(Space::new(12.0, 12.0))
                        .width(Length::Fixed(12.0))
                        .height(Length::Fixed(12.0))
                        .style(|_t| iced::widget::container::Style {
                            background: Some(theme::bg(theme::MUTED)),
                            border: iced::Border {
                                color: theme::MUTED,
                                width: 1.0,
                                radius: 6.0.into(),
                            },
                            ..Default::default()
                        }),
                    text("已暂停").size(12).color(theme::WARN),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            }]
            .align_y(Alignment::Center),
        )
        .on_press(Message::ToggleTracking)
        .style(move |_t, _s| iced::widget::button::Style {
            background: Some(theme::bg(if self.state.tracking {
                Color::from_rgb(0.08, 0.20, 0.16)
            } else {
                Color::from_rgb(0.12, 0.15, 0.19)
            })),
            border: iced::Border {
                color: if self.state.tracking {
                    Color::from_rgba(0.18, 0.82, 0.58, 0.4)
                } else {
                    Color::from_rgba(0.6, 0.6, 0.6, 0.2)
                },
                width: 1.0,
                radius: 14.0.into(),
            },
            ..Default::default()
        })
        .padding(pad(5.0, 12.0, 5.0, 12.0));

        let tracking_row = row![
            icons::icon(Icon::Play, theme::ACCENT, 18.0),
            column![
                body("剪贴板实时追踪")
                    .size(13)
                    .font(crate::font::name_font()),
                meta("开启后自动记录文本复制与粘贴去向；暂停后不记录新活动")
                    .size(11)
                    .color(theme::MUTED),
            ]
            .spacing(2)
            .width(Length::Fill),
            toggle_switch,
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let retention_row = row![
            icons::icon(Icon::History, theme::ACCENT, 18.0),
            column![
                body("历史数据保留期限")
                    .size(13)
                    .font(crate::font::name_font()),
                meta("超过此时长的历史记录与去向链将被自动彻底清理")
                    .size(11)
                    .color(theme::MUTED),
            ]
            .spacing(2)
            .width(Length::Fill),
            row![
                chip(Retention::Day1, self.state.retention),
                chip(Retention::Day7, self.state.retention),
                chip(Retention::Day30, self.state.retention),
                chip(Retention::Never, self.state.retention),
            ]
            .spacing(6),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let mut excluded_chips = row![].spacing(6);
        if self.state.excluded_apps.is_empty() {
            excluded_chips = excluded_chips.push(
                meta("暂无排除应用（全部应用均可被记录）")
                    .size(11)
                    .color(theme::FAINT),
            );
        } else {
            for exe in &self.state.excluded_apps {
                excluded_chips = excluded_chips.push(
                    container(
                        row![
                            text(exe).size(11).color(theme::INK),
                            button(icons::icon(Icon::Close, theme::DANGER, 11.0))
                                .on_press(Message::Unexclude(exe.clone()))
                                .style(theme::win_button)
                                .padding(2),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding(pad(4.0, 8.0, 4.0, 8.0))
                    .style(theme::chip_container),
                );
            }
        }

        let exclude_input_box: Element<'_, Message> = if self.hotkey_recording {
            meta("录制快捷键期间暂停排除应用输入").size(11).into()
        } else {
            row![
                text_input("输入应用文件名，如 app.exe", &self.exclude_input)
                    .on_input(Message::ExcludeInput)
                    .on_submit(Message::ExcludeSubmit)
                    .padding(pad(6.0, 10.0, 6.0, 10.0))
                    .size(12)
                    .style(theme::input_style)
                    .width(Length::Fixed(240.0)),
                button(
                    row![
                        icons::icon(Icon::Pin, theme::MUTED, 12.0),
                        text("添加排除").size(12).color(theme::INK)
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::ExcludeSubmit)
                .style(theme::action_button)
                .padding(pad(6.0, 12.0, 6.0, 12.0)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        };

        let excluded_block = column![
            row![
                icons::icon(Icon::Close, theme::WARN, 18.0),
                column![
                    body("隐私排除应用名单")
                        .size(13)
                        .font(crate::font::name_font()),
                    meta("加入后不记录该应用的新复制；已有历史不会自动删除")
                        .size(11)
                        .color(theme::MUTED),
                ]
                .spacing(2)
                .width(Length::Fill),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            Space::with_height(10.0),
            excluded_chips,
            Space::with_height(10.0),
            exclude_input_box,
        ]
        .spacing(0);

        // Group 1: Privacy & Tracking Card
        let group_privacy = container(
            column![
                tracking_row,
                Space::with_height(12.0),
                theme::hdivider(),
                Space::with_height(12.0),
                retention_row,
                Space::with_height(12.0),
                theme::hdivider(),
                Space::with_height(12.0),
                excluded_block,
            ]
            .spacing(0),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::panel_style);

        let (hotkey_status, hotkey_color) = if let Some(error) = &self.hotkey_record_error {
            (error.clone(), theme::DANGER)
        } else if self.hotkey_recording {
            (
                "请按新的组合键；Esc 取消。排除应用输入框已失焦".to_string(),
                theme::ACCENT,
            )
        } else if let Some(error) = &self.state.hotkey_error {
            (error.clone(), theme::DANGER)
        } else if self.state.hotkey_selected == Hotkey::Disabled {
            ("窗口快捷键已关闭".to_string(), theme::MUTED)
        } else if self.state.hotkey_selected == self.state.hotkey_active {
            (format!("{} 已生效", self.state.hotkey_active), theme::OK)
        } else {
            ("正在注册快捷键…".to_string(), theme::MUTED)
        };
        let hotkey_group = container(
            row![
                icons::icon(Icon::Search, theme::ACCENT, 18.0),
                column![
                    body("显示 / 隐藏 Veya")
                        .size(13)
                        .font(crate::font::name_font()),
                    meta(hotkey_status).size(11).color(hotkey_color),
                ]
                .spacing(2)
                .width(Length::Fill),
                container(
                    meta(self.state.hotkey_selected.to_string())
                        .size(12)
                        .color(theme::INK)
                )
                .padding(pad(5.0, 8.0, 5.0, 8.0))
                .style(theme::chip_container),
                button(
                    text(if self.hotkey_recording {
                        "取消"
                    } else {
                        "更改"
                    })
                    .size(12)
                )
                .on_press(if self.hotkey_recording {
                    Message::CancelHotkeyRecord
                } else {
                    Message::StartHotkeyRecord
                })
                .style(theme::action_button)
                .padding(pad(6.0, 10.0, 6.0, 10.0)),
                button(text("默认").size(12))
                    .on_press(Message::SetHotkey(Hotkey::ALT_V))
                    .style(theme::action_button)
                    .padding(pad(6.0, 10.0, 6.0, 10.0)),
                button(text("关闭").size(12))
                    .on_press(Message::SetHotkey(Hotkey::Disabled))
                    .style(theme::action_button)
                    .padding(pad(6.0, 10.0, 6.0, 10.0)),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::panel_style);

        // Group 2: Data & Danger Zone
        let data_stats_row = row![
            icons::icon(Icon::External, theme::ACCENT, 18.0),
            column![
                body("本地存储引擎").size(13).font(crate::font::name_font()),
                meta("全部数据仅保存在本机 SQLite 数据库，离线可用，绝不上传云端")
                    .size(11)
                    .color(theme::MUTED),
            ]
            .spacing(2)
            .width(Length::Fill),
            container(
                meta(format!("{} 条有效记录", self.state.record_count))
                    .size(12)
                    .color(theme::INK)
            )
            .padding(pad(4.0, 10.0, 4.0, 10.0))
            .style(theme::chip_container),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let clear_row = row![
            icons::icon(Icon::Trash, theme::DANGER, 18.0),
            column![
                body("清空全部历史记录")
                    .size(13)
                    .font(crate::font::name_font()),
                meta("将删除所有剪贴板记录与粘贴流向，包括已固定记录（操作不可逆）")
                    .size(11)
                    .color(theme::DANGER),
            ]
            .spacing(2)
            .width(Length::Fill),
            button(
                row![
                    icons::icon(Icon::Trash, theme::DANGER, 13.0),
                    text("清空全部数据").size(12).color(theme::DANGER)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::RequestClearHistory)
            .style(theme::danger_button)
            .padding(pad(7.0, 14.0, 7.0, 14.0)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let group_data = container(
            column![
                data_stats_row,
                Space::with_height(12.0),
                theme::hdivider(),
                Space::with_height(12.0),
                clear_row,
            ]
            .spacing(0),
        )
        .padding(18)
        .width(Length::Fill)
        .style(theme::panel_style);

        let busy = matches!(
            self.updates.status,
            update::Status::Checking | update::Status::Downloading
        );
        let can_install = matches!(self.updates.status, update::Status::Available(_))
            && self.updates.installer.is_some();
        let button_label = match &self.updates.status {
            update::Status::Checking => "检查中…",
            update::Status::Downloading => "下载中…",
            _ if can_install => "下载并安装",
            _ => "检查",
        };
        let update_button = button(text(button_label).size(12))
            .style(theme::action_button)
            .padding(pad(6.0, 12.0, 6.0, 12.0));
        let update_button = if busy {
            update_button
        } else if can_install {
            update_button.on_press(Message::DownloadUpdate)
        } else {
            update_button.on_press(Message::CheckUpdate)
        };
        let release_button = button(text("打开发布页").size(12))
            .on_press(Message::OpenReleases)
            .style(theme::action_button)
            .padding(pad(6.0, 12.0, 6.0, 12.0));

        // Group 3: About and user-requested update check.
        let about_group = container(
            column![
                row![
                    icons::icon(Icon::History, theme::ACCENT, 20.0),
                    column![
                        body(format!("Veya v{}", env!("CARGO_PKG_VERSION")))
                            .size(13)
                            .font(crate::font::name_font()),
                        meta("Clipboard Flow Tracker · 记住从哪来，去向何处。")
                            .size(11)
                            .color(theme::MUTED),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                Space::with_height(12.0),
                theme::hdivider(),
                Space::with_height(12.0),
                row![
                    icons::icon(Icon::Search, theme::ACCENT, 18.0),
                    column![
                        body("检查更新").size(13).font(crate::font::name_font()),
                        meta(self.updates.description())
                            .size(11)
                            .color(theme::MUTED),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    update_button,
                    release_button,
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                Space::with_height(12.0),
                theme::hdivider(),
                Space::with_height(12.0),
                row![
                    icons::icon(Icon::External, theme::ACCENT, 18.0),
                    column![
                        body("GitHub 仓库").size(13).font(crate::font::name_font()),
                        meta("github.com/NGLSL/Veya").size(11).color(theme::MUTED),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    button(text("访问仓库").size(12))
                        .on_press(Message::OpenRepository)
                        .style(theme::action_button)
                        .padding(pad(6.0, 12.0, 6.0, 12.0)),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(0),
        )
        .padding(16)
        .width(Length::Fill)
        .style(theme::inset_style);

        // Restrain content inside max width container (660px) to prevent horizontal stretching
        let settings_content = column![
            section_label("隐私与剪贴板追踪"),
            Space::with_height(8.0),
            group_privacy,
            Space::with_height(20.0),
            section_label("窗口与快捷键"),
            Space::with_height(8.0),
            hotkey_group,
            Space::with_height(20.0),
            section_label("数据管理与危险操作"),
            Space::with_height(8.0),
            group_data,
            Space::with_height(20.0),
            section_label("关于 Veya"),
            Space::with_height(8.0),
            about_group,
            Space::with_height(32.0),
        ]
        .spacing(0)
        .width(Length::Fixed(660.0));

        scrollable(
            container(settings_content)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding(pad(10.0, 0.0, 30.0, 0.0)),
        )
        .style(theme::scroll_style)
        .height(Length::Fill)
        .into()
    }

    pub(super) fn clear_history_confirmation_modal(&self) -> Element<'_, Message> {
        let dialog =
            container(
                column![
                    row![
                        icons::icon(Icon::Trash, theme::DANGER, 20.0),
                        column![
                        body("清空全部历史记录？")
                            .size(15)
                            .font(crate::font::name_font()),
                        meta("所有剪贴板记录、粘贴流向和已固定记录都会被永久删除。此操作无法撤销。")
                            .size(12)
                            .color(theme::MUTED),
                    ]
                        .spacing(5)
                        .width(Length::Fill),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                    theme::hdivider(),
                    row![
                        Space::with_width(Length::Fill),
                        button(meta("取消").size(12).color(theme::MUTED))
                            .on_press(Message::CancelClearHistory)
                            .style(theme::action_button)
                            .padding(pad(8.0, 14.0, 8.0, 14.0)),
                        button(
                            row![
                                icons::icon(Icon::Trash, theme::DANGER, 13.0),
                                text("清空全部").size(12).color(theme::DANGER),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::ConfirmClearHistory)
                        .style(theme::danger_button)
                        .padding(pad(8.0, 14.0, 8.0, 14.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(16),
            )
            .width(Length::Fill)
            .max_width(480)
            .padding(20)
            .style(theme::modal_panel_style);

        mouse_area(
            container(opaque(dialog))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .padding(28)
                .style(theme::modal_backdrop_style),
        )
        .on_press(Message::CancelClearHistory)
        .into()
    }
}
