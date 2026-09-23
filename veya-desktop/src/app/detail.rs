use super::*;
use iced::widget::column;

impl App {
    pub(super) fn detail_panel(&self) -> Element<'_, Message> {
        let Some(seq) = self.state.selected else {
            return container(
                column![
                    icons::icon(Icon::Copy, theme::FAINT, 28.0),
                    Space::with_height(10.0),
                    body("选择一条记录查看详情")
                        .size(14)
                        .font(crate::font::name_font()),
                    Space::with_height(4.0),
                    meta("复制来源、使用时间线会出现在这里。")
                        .size(11)
                        .color(theme::FAINT),
                ]
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(theme::pad_panel())
            .style(theme::panel_style)
            .into();
        };
        let Some(card) = self.state.cards.iter().find(|c| c.sequence == seq) else {
            return Space::new(Length::Fill, Length::Fill).into();
        };

        let (copy_label, copy_icon, copy_color) = if self.copied_tick > 0 {
            ("已复制", Icon::Check, theme::OK)
        } else {
            ("复制", Icon::Copy, theme::INK)
        };

        // Semantic header summary (avoids 100% duplicating full content & word wrapping breaks)
        let header_title = detail_title(card);
        let header_sub = detail_subtitle(card);

        let title = row![
            detail_type_visual(card, 46.0),
            column![
                text(header_title)
                    .size(16)
                    .color(theme::INK)
                    .font(crate::font::name_font())
                    .shaping(text::Shaping::Advanced),
                meta(header_sub).size(12).color(theme::MUTED),
            ]
            .spacing(3)
            .width(Length::Fill),
            button(
                row![
                    icons::icon(copy_icon, copy_color, 14.0),
                    text(copy_label).size(13).color(copy_color)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::Copy(card.sequence))
            .style(theme::primary_button)
            .padding(pad(8.0, 16.0, 8.0, 16.0)),
            button(icons::icon(Icon::More, theme::MUTED, 16.0))
                .on_press(Message::ToggleDetailMenu)
                .style(theme::action_button)
                .padding(pad(8.0, 10.0, 8.0, 10.0)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        // A short, read-only preview; the full content opens in a separate dialog.
        let is_link = card.kind == ContentKind::Link;
        let is_text_kind = matches!(
            card.kind,
            ContentKind::Text | ContentKind::Link | ContentKind::Code
        );
        let url = card.full_content.trim().to_string();
        let (preview, has_more) = detail_content_preview(&card.full_content);
        let mut actions = row![].spacing(6);

        if is_link {
            actions = actions.push(
                button(
                    row![
                        icons::icon(Icon::External, theme::MUTED, 14.0),
                        meta("打开链接").size(12).color(theme::MUTED)
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenLink(url.clone()))
                .style(theme::action_button)
                .padding(pad(6.0, 10.0, 6.0, 10.0)),
            );
        }

        if is_text_kind {
            actions = actions.push(
                button(
                    row![
                        icons::icon(Icon::Search, theme::MUTED, 14.0),
                        meta("搜索").size(12).color(theme::MUTED)
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::WebSearch(card.full_content.clone()))
                .style(theme::action_button)
                .padding(pad(6.0, 10.0, 6.0, 10.0)),
            );
        }

        if matches!(card.payload, CardPayloadView::Image { .. }) {
            actions = actions.push(
                button(meta("查看图片").size(12).color(theme::MUTED))
                    .on_press(Message::OpenContentModal(card.sequence))
                    .style(theme::action_button)
                    .padding(pad(6.0, 10.0, 6.0, 10.0)),
            );
        }

        actions = actions.push(
            button(
                row![
                    icons::icon(Icon::Copy, theme::MUTED, 14.0),
                    meta("复制为纯文本").size(12).color(theme::MUTED)
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::CopyPlainText(card.sequence))
            .style(theme::action_button)
            .padding(pad(6.0, 10.0, 6.0, 10.0)),
        );

        if is_text_kind {
            actions = actions.push(
                button(
                    row![
                        icons::icon(Icon::QrCode, theme::MUTED, 14.0),
                        meta("二维码").size(12).color(theme::MUTED)
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::ToggleQrCode)
                .style(theme::action_button)
                .padding(pad(6.0, 10.0, 6.0, 10.0)),
            );
        }

        if has_more {
            actions = actions.push(Space::with_width(Length::Fill)).push(
                button(meta("展开全文 ↗").size(12).color(theme::ACCENT_BORDER))
                    .on_press(Message::OpenContentModal(card.sequence))
                    .style(theme::toolbar_filter_button(false))
                    .padding(pad(6.0, 8.0, 6.0, 8.0)),
            );
        }

        let preview_content: Element<'_, Message> = match &card.payload {
            crate::capture::CardPayloadView::Text => column![
                meta("内容预览").size(11).color(theme::FAINT),
                text(preview)
                    .size(13)
                    .color(theme::INK)
                    .font(crate::font::ui_font())
                    .shaping(text::Shaping::Advanced)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(7)
            .into(),
            crate::capture::CardPayloadView::Files(paths) => {
                let mut file_rows = column![meta(format!("本地文件与文件夹 · {} 项", paths.len()))
                    .size(11)
                    .color(theme::FAINT),]
                .spacing(6);
                for path in paths {
                    file_rows = file_rows.push(
                        row![
                            icons::icon(Icon::File, theme::ACCENT, 13.0),
                            text(path.clone())
                                .size(11)
                                .color(theme::INK)
                                .font(crate::font::ui_font())
                                .shaping(text::Shaping::Advanced)
                                .width(Length::Fill),
                        ]
                        .spacing(7)
                        .align_y(Alignment::Center),
                    );
                }
                scrollable(file_rows)
                    .style(theme::scroll_style)
                    .height(Length::Fixed(92.0))
                    .into()
            }
            crate::capture::CardPayloadView::Image {
                handle,
                width,
                height,
                encoded_bytes,
            } => row![
                mouse_area(
                    container(
                        iced::widget::image(handle.clone())
                            .width(Length::Fixed(156.0))
                            .height(Length::Fixed(112.0)),
                    )
                    .width(Length::Fixed(164.0))
                    .height(Length::Fixed(120.0))
                    .center_x(Length::Fixed(164.0))
                    .center_y(Length::Fixed(120.0))
                    .clip(true)
                    .style(theme::inset_style),
                )
                .on_press(Message::OpenContentModal(card.sequence)),
                column![
                    meta("图片预览").size(11).color(theme::FAINT),
                    body(format!("{width} × {height} px"))
                        .size(13)
                        .font(crate::font::name_font()),
                    meta(format::byte_size_label(*encoded_bytes))
                        .size(11)
                        .color(theme::MUTED),
                ]
                .spacing(5)
                .width(Length::Fill),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        };

        let mut content_inner = column![
            container(preview_content)
                .padding(pad(3.0, 2.0, 11.0, 2.0))
                .width(Length::Fill)
                .clip(true),
            theme::hdivider(),
            container(actions).padding(pad(7.0, 0.0, 0.0, 0.0)),
        ]
        .spacing(0);

        if self.show_qrcode && is_text_kind {
            content_inner = content_inner.push(theme::hdivider()).push(
                container(
                    row![
                        icons::icon(Icon::QrCode, theme::INK, 56.0),
                        column![
                            body("手机扫码跨端读取")
                                .size(13)
                                .font(crate::font::name_font()),
                            meta("微信扫一扫、系统相机或手机浏览器直接使用")
                                .size(11)
                                .color(theme::MUTED),
                        ]
                        .spacing(3),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center),
                )
                .padding(12)
                .width(Length::Fill)
                .style(theme::inset_style),
            );
        }

        let content_box = container(content_inner).width(Length::Fill);

        // Source card (Fig 1: 「复制来源」 with clean path and non-redundant context preview)
        let source_title =
            format::source_label(&short_app(&card.source_app), card.source_confidence);
        let hint = format::confidence_hint(card.source_confidence);
        let raw_exe_path = self.source_path_for(&card.source_app);
        let short_path = truncate_path(&raw_exe_path, 34);

        let mut source_col = column![
            section_label("复制来源"),
            Space::with_height(10.0),
            row![
                icons::app_avatar::<Message>(&card.source_app, 38.0),
                column![
                    body(source_title).size(14).font(crate::font::name_font()),
                    meta(short_path).size(11).color(theme::FAINT),
                    meta(card.time_full.clone()).size(11).color(theme::FAINT),
                ]
                .spacing(2)
                .width(Length::Fill),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(0);

        // Embedded context card (web preview / window title matching Fig 1 without double host)
        let context_info = if matches!(&card.payload, crate::capture::CardPayloadView::Text) {
            extract_context_preview(&card.full_content, &card.source_window)
        } else {
            None
        };
        if let Some((icon_kind, ctx_title, ctx_sub)) = context_info {
            source_col = source_col.push(Space::with_height(12.0)).push(
                container(
                    row![
                        icons::icon(icon_kind, theme::ACCENT, 18.0),
                        column![
                            body(ctx_title).size(12).font(crate::font::name_font()),
                            meta(ctx_sub).size(11).color(theme::FAINT),
                        ]
                        .spacing(1)
                        .width(Length::Fill),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding(8)
                .width(Length::Fill)
                .style(theme::inset_style),
            );
        }

        if !hint.is_empty() {
            source_col = source_col.push(Space::with_height(6.0));
            source_col = source_col.push(meta(hint).size(11).color(theme::WARN));
        }

        // Usage timeline (Fig 1: 「使用记录」 with vertical connecting line and filtered '-' chars)
        let usage_title = if card.has_paste_activity {
            format!("使用记录 ({} 次)", card.used_in.len())
        } else {
            "使用记录".to_string()
        };
        let mut usage = column![section_label(usage_title), Space::with_height(10.0),].spacing(0);

        if card.has_paste_activity {
            let n = card.used_in.len();
            for (i, u) in card.used_in.iter().enumerate() {
                let last = i + 1 == n;
                usage = usage.push(timeline_row::<Message>(
                    &u.time_label,
                    &u.target_app,
                    &u.method_label,
                    &u.target_window,
                    i == 0,
                    last,
                ));
            }
            usage = usage.push(meta(card.paste_detail).size(10).color(theme::FAINT));
        } else {
            usage = usage.push(meta("暂无粘贴记录").size(12).color(theme::FAINT));
        }

        // Content type section at the bottom of the right card
        let kind_block = column![
            Space::with_height(10.0),
            theme::hdivider(),
            Space::with_height(8.0),
            section_label("内容类型"),
            Space::with_height(5.0),
            row![
                icons::type_chip::<Message>(card.kind, 30.0),
                column![
                    body(card.kind.label())
                        .size(13)
                        .font(crate::font::name_font()),
                    meta(card.kind.hint()).size(11).color(theme::FAINT),
                ]
                .spacing(1),
            ]
            .spacing(9)
            .align_y(Alignment::Center),
        ]
        .spacing(0);

        // Lower dual-card layout: 270px fixed height to prevent vertical overflow and extra scrollbar
        let lower = row![
            container(source_col)
                .padding(14)
                .width(Length::FillPortion(1))
                .height(Length::Fixed(270.0))
                .style(theme::panel_style),
            container(
                column![
                    scrollable(usage).style(theme::scroll_style),
                    Space::with_height(Length::Fill),
                    kind_block,
                ]
                .spacing(0)
            )
            .padding(14)
            .width(Length::FillPortion(1))
            .height(Length::Fixed(270.0))
            .style(theme::panel_style),
        ]
        .spacing(12);

        let footer = row![
            Space::with_width(Length::Fill),
            meta(format!("序列号 #{}", card.sequence))
                .size(11)
                .color(theme::FAINT),
        ];

        let exe = base_exe(&card.source_app).to_string();
        let mut more = row![
            button(
                row![
                    icons::icon(
                        Icon::Pin,
                        if card.pinned {
                            theme::ACCENT
                        } else {
                            theme::MUTED
                        },
                        12.0
                    ),
                    meta(if card.pinned {
                        "取消固定"
                    } else {
                        "固定记录"
                    })
                    .size(11),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::SetPinned {
                sequences: card.raw_sequences.clone(),
                pinned: !card.pinned,
            })
            .style(theme::action_button)
            .padding(theme::pad_narrow()),
            button(
                row![
                    icons::icon(Icon::Terminal, theme::MUTED, 12.0),
                    meta(if self.state.expanded_raw {
                        "收起原始"
                    } else {
                        "原始事件"
                    })
                    .size(11),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ToggleRaw)
            .style(theme::action_button)
            .padding(theme::pad_narrow()),
            button(
                row![
                    icons::icon(Icon::Close, theme::MUTED, 12.0),
                    meta("排除应用").size(11)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ExcludeSource(exe.clone()))
            .style(theme::action_button)
            .padding(theme::pad_narrow()),
            button(
                row![
                    icons::icon(Icon::Trash, theme::DANGER, 12.0),
                    text("删除").size(11).color(theme::DANGER),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::Delete(card.raw_sequences.clone()))
            .style(theme::danger_button)
            .padding(theme::pad_narrow()),
        ]
        .spacing(4);
        if !exe.is_empty() && exe != "unknown" {
            more = more.push(
                button(
                    row![
                        icons::icon(Icon::External, theme::MUTED, 12.0),
                        meta("打开来源").size(11)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenSource(raw_exe_path))
                .style(theme::action_button)
                .padding(theme::pad_narrow()),
            );
        }

        let mut detail = column![
            title,
            Space::with_height(12.0),
            content_box,
            Space::with_height(12.0),
        ]
        .spacing(0);

        if self.detail_menu_open {
            detail = detail.push(
                container(more)
                    .padding(pad(4.0, 8.0, 4.0, 8.0))
                    .style(theme::inset_style),
            );
            detail = detail.push(Space::with_height(10.0));
        }
        if self.state.expanded_raw {
            let sequences = card
                .raw_sequences
                .iter()
                .map(|sequence| format!("#{sequence}"))
                .collect::<Vec<_>>()
                .join(" · ");
            detail = detail.push(
                container(
                    column![
                        section_label("原始剪贴板事件"),
                        meta(sequences).size(11).color(theme::FAINT),
                    ]
                    .spacing(5),
                )
                .padding(pad(8.0, 10.0, 8.0, 10.0))
                .width(Length::Fill)
                .style(theme::inset_style),
            );
            detail = detail.push(Space::with_height(10.0));
        }

        detail = detail
            .push(lower)
            .push(Space::with_height(10.0))
            .push(footer);

        // Crisp flat container avoiding double scrollbar
        container(detail)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(theme::pad_panel())
            .style(theme::panel_style)
            .into()
    }

    pub(super) fn content_modal(&self, card: &CardView) -> Element<'_, Message> {
        let copy_label = if self.copied_tick > 0 {
            "已复制"
        } else {
            if card.kind == ContentKind::Image {
                "复制图片"
            } else {
                "复制全文"
            }
        };
        let header = row![
            column![
                body(if card.kind == ContentKind::Image {
                    "查看图片"
                } else {
                    "查看完整内容"
                })
                .size(15)
                .font(crate::font::name_font()),
                meta(format!("{} · 记录 #{}", card.kind.label(), card.sequence))
                    .size(11)
                    .color(theme::FAINT),
            ]
            .spacing(3)
            .width(Length::Fill),
            button(icons::icon(Icon::Close, theme::MUTED, 16.0))
                .on_press(Message::CloseContentModal)
                .style(theme::action_button)
                .padding(8),
        ]
        .align_y(Alignment::Center);

        let footer = row![
            Space::with_width(Length::Fill),
            button(meta("关闭").size(12).color(theme::MUTED))
                .on_press(Message::CloseContentModal)
                .style(theme::action_button)
                .padding(pad(8.0, 13.0, 8.0, 13.0)),
            button(
                row![
                    icons::icon(Icon::Copy, theme::INK, 14.0),
                    text(copy_label).size(12).color(theme::INK),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::Copy(card.sequence))
            .style(theme::primary_button)
            .padding(pad(8.0, 14.0, 8.0, 14.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let is_code = card.kind == ContentKind::Code;
        let content_font = if is_code {
            iced::Font::MONOSPACE
        } else {
            crate::font::ui_font()
        };
        let content_scroll = scrollable(
            container(
                text(card.full_content.clone())
                    .size(13)
                    .line_height(iced::widget::text::LineHeight::Absolute(20.0.into()))
                    .color(theme::INK)
                    .font(content_font)
                    .shaping(text::Shaping::Advanced)
                    .width(if is_code {
                        Length::Shrink
                    } else {
                        Length::Fill
                    })
                    .wrapping(if is_code {
                        text::Wrapping::None
                    } else {
                        text::Wrapping::WordOrGlyph
                    }),
            )
            .width(if is_code {
                Length::Shrink
            } else {
                Length::Fill
            })
            .padding(pad(13.0, 15.0, 13.0, 15.0)),
        )
        .direction(if is_code {
            scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::new(),
                horizontal: scrollable::Scrollbar::new(),
            }
        } else {
            scrollable::Direction::Vertical(scrollable::Scrollbar::new())
        })
        .style(theme::scroll_style)
        .height(Length::Fill);
        let content: Element<'_, Message> =
            if let CardPayloadView::Image { handle, .. } = &card.payload {
                container(
                    iced::widget::image::viewer(handle.clone())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced::ContentFit::Contain),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::modal_content_style)
                .into()
            } else {
                container(content_scroll)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme::modal_content_style)
                    .into()
            };

        let dialog = container(column![header, content, footer,].spacing(15))
            .width(Length::Fill)
            .max_width(if card.kind == ContentKind::Image {
                960
            } else {
                680
            })
            .height(Length::Fill)
            .max_height(if card.kind == ContentKind::Image {
                640
            } else {
                460
            })
            .padding(18)
            .style(theme::modal_panel_style);

        mouse_area(
            container(opaque(dialog))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .padding(36)
                .style(theme::modal_backdrop_style),
        )
        .on_press(Message::CloseContentModal)
        .into()
    }
}

fn detail_type_visual<'a>(card: &'a CardView, size: f32) -> Element<'a, Message> {
    if let crate::capture::CardPayloadView::Image { handle, .. } = &card.payload {
        let inner = (size - 4.0).max(1.0);
        container(
            iced::widget::image(handle.clone())
                .width(Length::Fixed(inner))
                .height(Length::Fixed(inner)),
        )
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .clip(true)
        .style(theme::inset_style)
        .into()
    } else {
        icons::detail_type_chip::<Message>(card.kind, size).into()
    }
}
