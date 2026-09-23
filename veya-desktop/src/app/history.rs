use super::*;
use iced::widget::column;

impl App {
    pub(super) fn history_toolbar(&self) -> Element<'_, Message> {
        let filters: [Filter; 6] = [
            Filter::All,
            Filter::Text,
            Filter::Link,
            Filter::Code,
            Filter::File,
            Filter::Image,
        ];
        let mut tabs = row![].spacing(6);
        for f in filters {
            let selected = self.filter == f;
            let icon_color = if selected { theme::INK } else { theme::MUTED };
            let tab = button(
                row![
                    icons::icon(f.icon(), icon_color, 13.0),
                    text(f.label()).size(13).color(icon_color)
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .style(theme::toolbar_filter_button(selected))
            .padding(pad(6.0, 11.0, 6.0, 11.0));
            tabs = tabs.push(tab.on_press(Message::SetFilter(f)));
        }

        let pin_color = if self.pinned_only {
            theme::INK
        } else {
            theme::MUTED
        };
        tabs = tabs.push(
            button(
                row![
                    icons::icon(Icon::Pin, pin_color, 13.0),
                    text("已固定").size(13).color(pin_color),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::TogglePinnedOnly)
            .style(theme::toolbar_filter_button(self.pinned_only))
            .padding(pad(6.0, 11.0, 6.0, 11.0)),
        );

        let sort = pick_list(
            [SortOrder::Newest, SortOrder::Oldest],
            Some(self.sort_order),
            Message::SetSortOrder,
        )
        .width(Length::Fixed(116.0))
        .padding(pad(6.0, 10.0, 6.0, 12.0))
        .text_size(12)
        .style(theme::sort_picker)
        .menu_style(theme::sort_menu);

        let density_hint = match self.list_density {
            ListDensity::Detailed => "当前：详细列表，点击切换紧凑列表",
            ListDensity::Compact => "当前：紧凑列表，点击切换详细列表",
        };
        let compact = self.list_density == ListDensity::Compact;
        let density_button = button(icons::icon(
            Icon::List,
            if compact { theme::INK } else { theme::MUTED },
            15.0,
        ))
        .on_press(Message::ToggleListDensity)
        .padding(8)
        .style(theme::chip_button(compact));

        let toolbar = row![
            tabs,
            Space::with_width(Length::Fill),
            sort,
            tooltip(
                density_button,
                meta(density_hint).size(12),
                tooltip::Position::Bottom
            )
            .style(theme::chip_container),
        ]
        .align_y(Alignment::Center)
        .spacing(8);

        container(toolbar)
            .padding(pad(6.0, 16.0, 6.0, 14.0))
            .width(Length::Fill)
            .into()
    }

    pub(super) fn history_column(&self) -> Element<'_, Message> {
        let filtered = self.filtered_cards();
        if filtered.is_empty() {
            let searching = !self.search.trim().is_empty();
            let (icon, title, sub) = if searching {
                (
                    Icon::Search,
                    "没有匹配结果",
                    "换个关键词，或清空搜索看看全部记录。",
                )
            } else if self.pinned_only {
                let hint = match self.filter {
                    Filter::File => "从资源管理器复制本地文件或文件夹，再固定记录。",
                    Filter::Image => "复制剪贴板中的图片，再固定记录。",
                    _ => "在记录详情中固定后，会显示在这里。",
                };
                (
                    Icon::Pin,
                    if self.filter == Filter::All {
                        "还没有固定记录"
                    } else {
                        "当前分类没有固定记录"
                    },
                    hint,
                )
            } else if self.filter != Filter::All {
                match self.filter {
                    Filter::File => (
                        Icon::File,
                        "还没有文件记录",
                        "从资源管理器复制本地文件或文件夹后，会显示在这里。",
                    ),
                    Filter::Image => (
                        Icon::Image,
                        "还没有图片记录",
                        "复制剪贴板中的图片后，会显示在这里。",
                    ),
                    _ => (
                        Icon::History,
                        "该分类暂无记录",
                        "选择其他分类，或复制相应内容后再查看。",
                    ),
                }
            } else {
                (
                    Icon::History,
                    "还没有剪贴板记录",
                    "复制点东西，Veya 会在这里记住它们。",
                )
            };
            return container(
                column![
                    icons::icon(icon, theme::FAINT, 28.0),
                    Space::with_height(10.0),
                    body(title).size(14).font(crate::font::name_font()),
                    Space::with_height(4.0),
                    meta(sub).size(13).color(theme::FAINT),
                ]
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        }

        // Clean seamless stream without intrusive date headers matching Fig 1
        let spacing = if self.list_density == ListDensity::Compact {
            2
        } else {
            6
        };
        let mut list = column![].spacing(spacing).padding(pad(6.0, 4.0, 6.0, 2.0));
        for card in &filtered {
            list = list.push(self.card_widget(card));
        }

        column![scrollable(list)
            .id(history_scroll_id())
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(4)
                    .scroller_width(4)
                    .margin(2),
            ))
            .style(theme::scroll_style)
            .height(Length::Fill)]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    pub(super) fn filtered_cards(&self) -> Vec<&CardView> {
        visible_cards(
            &self.state.cards,
            self.filter,
            self.pinned_only,
            self.sort_order,
            &self.search,
        )
    }

    pub(super) fn ensure_visible_selection(&mut self) {
        let visible = self.filtered_cards();
        let current = self.state.selected;
        let next = current
            .filter(|seq| visible.iter().any(|card| card.sequence == *seq))
            .or_else(|| visible.first().map(|card| card.sequence));
        if current != next {
            self.state.selected = next;
            self.detail_menu_open = false;
            self.content_modal_for = None;
            self.show_qrcode = false;
        }
    }

    pub(super) fn card_widget<'a>(&self, card: &'a CardView) -> Element<'a, Message> {
        let selected = self.state.selected == Some(card.sequence);

        if self.list_density == ListDensity::Compact {
            let preview = compact_preview(&card.content_preview, 26);
            let mut source_meta = row![
                icons::app_avatar::<Message>(&card.source_app, 13.0),
                meta(format!(
                    "{} · {}",
                    short_app(&card.source_app),
                    card.relative_time
                ))
                .size(10)
                .color(theme::MUTED),
            ]
            .spacing(5)
            .align_y(Alignment::Center);
            if card.pinned {
                source_meta = source_meta.push(icons::icon(Icon::Pin, theme::ACCENT, 10.0));
            }
            let head = row![
                card_leading_visual(card, 29.0),
                column![
                    body(preview)
                        .size(12)
                        .font(crate::font::name_font())
                        .shaping(text::Shaping::Advanced),
                    source_meta,
                ]
                .spacing(2)
                .width(Length::Fill),
            ]
            .spacing(9)
            .align_y(Alignment::Center);
            let card_button = button(head)
                .on_press(Message::Select(card.sequence))
                .width(Length::Fill)
                .padding(pad(6.0, 10.0, 6.0, 10.0))
                .style(theme::card_button(selected))
                .into();
            return self.card_with_right_click(card, card_button);
        }

        let time_bit = if card.raw_count > 1 && !card.time_range.is_empty() {
            format!("{} 次复制 · {}", card.raw_count, card.time_range)
        } else {
            format!("{} · {}", short_app(&card.source_app), card.relative_time)
        };

        let used_line = if card.has_paste_activity {
            let mut parts: Vec<String> = card
                .used_in_apps
                .iter()
                .take(2)
                .map(|app| short_app(app))
                .collect();
            let extra = card.used_in_apps.len().saturating_sub(2);
            if extra > 0 {
                parts.push(format!("+{extra}"));
            }
            format!("→  {}", parts.join(" · "))
        } else {
            "→  暂无粘贴记录".to_string()
        };

        let mut source_meta = row![
            icons::app_avatar::<Message>(&card.source_app, 15.0),
            meta(time_bit).size(11).color(theme::MUTED),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if card.pinned {
            source_meta = source_meta.push(icons::icon(Icon::Pin, theme::ACCENT, 11.0));
        }

        let head = row![
            card_leading_visual(card, 38.0),
            column![
                body(card.content_preview.clone())
                    .size(13)
                    .font(crate::font::name_font())
                    .shaping(text::Shaping::Advanced),
                source_meta,
                meta(used_line).size(11).color(if card.has_paste_activity {
                    theme::MUTED
                } else {
                    theme::FAINT
                }),
            ]
            .spacing(3)
            .width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let card_button = button(head)
            .on_press(Message::Select(card.sequence))
            .width(Length::Fill)
            .padding(pad(9.0, 12.0, 9.0, 12.0))
            .style(theme::card_button(selected))
            .into();
        self.card_with_right_click(card, card_button)
    }

    fn card_with_right_click<'a>(
        &self,
        card: &'a CardView,
        card_button: Element<'a, Message>,
    ) -> Element<'a, Message> {
        mouse_area(card_button)
            .on_right_press(Message::OpenCardMenu(card.sequence))
            .into()
    }

    pub(super) fn card_context_menu(
        &self,
        card: &CardView,
        position: iced::Point,
    ) -> Element<'_, Message> {
        let pin_label = if card.pinned {
            "取消固定"
        } else {
            "固定"
        };
        let pin = button(
            row![
                icons::icon(Icon::Pin, theme::MUTED, 14.0),
                text(pin_label).size(12),
            ]
            .spacing(9)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(pad(8.0, 10.0, 8.0, 10.0))
        .on_press(Message::SetPinned {
            sequences: card.raw_sequences.clone(),
            pinned: !card.pinned,
        })
        .style(theme::context_menu_item(false));
        let delete = button(
            row![
                icons::icon(Icon::Trash, theme::DANGER, 14.0),
                text("删除").size(12),
            ]
            .spacing(9)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(pad(8.0, 10.0, 8.0, 10.0))
        .on_press(Message::Delete(card.raw_sequences.clone()))
        .style(theme::context_menu_item(true));

        let menu = mouse_area(
            container(column![pin, delete].spacing(2))
                .width(Length::Fixed(170.0))
                .padding(4)
                .style(theme::context_menu_panel),
        )
        .on_press(Message::CloseCardMenu)
        .on_right_press(Message::CloseCardMenu);
        let origin = card_menu_origin(position, self.window_size);
        let positioned = container(menu)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .padding(iced::Padding {
                top: origin.y,
                left: origin.x,
                right: 0.0,
                bottom: 0.0,
            });
        let dismiss =
            mouse_area(Space::new(Length::Fill, Length::Fill)).on_press(Message::CloseCardMenu);
        stack([dismiss.into(), positioned.into()]).into()
    }
}

fn card_menu_origin(cursor: iced::Point, window: iced::Size) -> iced::Point {
    const MENU_WIDTH: f32 = 170.0;
    const MENU_HEIGHT: f32 = 82.0;
    let max_x = (window.width - MENU_WIDTH - 8.0).max(0.0);
    let max_y = (window.height - MENU_HEIGHT - 8.0).max(0.0);
    iced::Point::new(cursor.x.clamp(0.0, max_x), cursor.y.clamp(0.0, max_y))
}

fn card_leading_visual<'a>(card: &'a CardView, size: f32) -> Element<'a, Message> {
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
        icons::type_chip::<Message>(card.kind, size).into()
    }
}
