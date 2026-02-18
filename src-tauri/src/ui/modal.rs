use eframe::egui;

use crate::i18n::{self, Language};

use super::style;

pub enum ModalState {
    None,
    CreateContainer {
        name: String,
        description: String,
    },
    ConfirmDelete {
        container_name: String,
    },
    ConfirmClear {
        container_name: String,
    },
    ConfirmReindex {
        container_name: String,
        folder_count: usize,
    },
}

impl ModalState {
    pub fn take_action(&mut self) -> Option<String> {
        None // Reserved for future keyboard-triggered modal actions
    }
}

pub enum ModalResult {
    None,
    CreateContainer { name: String, description: String },
    ConfirmDelete,
    ConfirmClear,
    ConfirmReindex,
}

pub fn show(ctx: &egui::Context, modal: &mut ModalState, locale: Language) -> ModalResult {
    let mut result = ModalResult::None;
    let mut close = false;

    match modal {
        ModalState::None => {}

        ModalState::CreateContainer { name, description } => {
            // Overlay
            let overlay = egui::Area::new(egui::Id::new("modal_overlay"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::LEFT_TOP, egui::vec2(0.0, 0.0));

            overlay.show(ctx, |ui| {
                let screen = ctx.viewport_rect();
                let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 115),
                );
            });

            egui::Window::new(i18n::ts(locale, "dialog_new_container"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .fixed_size(egui::vec2(340.0, 0.0))
                .show(ctx, |ui| {
                    ui.add_space(8.0);

                    // Name field
                    ui.label(
                        egui::RichText::new(i18n::ts(locale, "dialog_field_name").to_uppercase())
                            .size(10.0)
                            .color(style::TEXT_TERTIARY)
                            .strong(),
                    );
                    let name_response = ui.add(
                        egui::TextEdit::singleline(name)
                            .hint_text(i18n::ts(locale, "dialog_field_name_placeholder"))
                            .desired_width(f32::INFINITY),
                    );
                    if name_response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !name.trim().is_empty()
                    {
                        result = ModalResult::CreateContainer {
                            name: name.trim().to_string(),
                            description: description.trim().to_string(),
                        };
                        close = true;
                    }

                    ui.add_space(8.0);

                    // Description field
                    ui.label(
                        egui::RichText::new(
                            i18n::ts(locale, "dialog_field_description").to_uppercase(),
                        )
                        .size(10.0)
                        .color(style::TEXT_TERTIARY)
                        .strong(),
                    );
                    ui.add(
                        egui::TextEdit::singleline(description)
                            .hint_text(i18n::ts(locale, "dialog_field_description_placeholder"))
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(12.0);

                    // Buttons
                    ui.horizontal(|ui| {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(i18n::ts(
                                                locale,
                                                "dialog_create",
                                            ))
                                            .color(egui::Color32::BLACK),
                                        )
                                        .fill(style::ACCENT),
                                    )
                                    .clicked()
                                    && !name.trim().is_empty()
                                {
                                    result = ModalResult::CreateContainer {
                                        name: name.trim().to_string(),
                                        description: description.trim().to_string(),
                                    };
                                    close = true;
                                }

                                if ui
                                    .button(i18n::ts(locale, "modal_cancel"))
                                    .clicked()
                                {
                                    close = true;
                                }
                            },
                        );
                    });
                });

            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
        }

        ModalState::ConfirmDelete { container_name } => {
            let msg = i18n::t(locale, "dialog_delete_message", &[("name", container_name)]);
            show_confirm_modal(
                ctx,
                locale,
                "dialog_delete_title",
                &msg,
                "dialog_delete_confirm",
                true,
                &mut result,
                &mut close,
                ModalResult::ConfirmDelete,
            );
        }

        ModalState::ConfirmClear { container_name } => {
            let msg = i18n::t(locale, "dialog_clear_message", &[("name", container_name)]);
            show_confirm_modal(
                ctx,
                locale,
                "dialog_clear_title",
                &msg,
                "dialog_clear_confirm",
                true,
                &mut result,
                &mut close,
                ModalResult::ConfirmClear,
            );
        }

        ModalState::ConfirmReindex { container_name, folder_count } => {
            let count_str = folder_count.to_string();
            let msg = i18n::t(
                locale,
                "dialog_rebuild_message",
                &[("name", container_name), ("count", &count_str)],
            );
            show_confirm_modal(
                ctx,
                locale,
                "dialog_rebuild_title",
                &msg,
                "dialog_rebuild_confirm",
                false,
                &mut result,
                &mut close,
                ModalResult::ConfirmReindex,
            );
        }
    }

    if close {
        *modal = ModalState::None;
    }

    result
}

fn show_confirm_modal(
    ctx: &egui::Context,
    locale: Language,
    title_key: &str,
    message: &str,
    confirm_key: &str,
    is_danger: bool,
    result: &mut ModalResult,
    close: &mut bool,
    on_confirm: ModalResult,
) {
    // Overlay
    let overlay = egui::Area::new(egui::Id::new("modal_overlay_confirm"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(0.0, 0.0));

    overlay.show(ctx, |ui| {
        let screen = ctx.viewport_rect();
        let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
        ui.painter().rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 115),
        );
    });

    egui::Window::new(i18n::ts(locale, title_key))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .fixed_size(egui::vec2(340.0, 0.0))
        .show(ctx, |ui| {
            ui.add_space(8.0);

            // Icon
            let icon_text = if is_danger { "\u{26A0}" } else { "\u{2139}" };
            let icon_color = if is_danger {
                egui::Color32::from_rgb(255, 200, 98)
            } else {
                egui::Color32::from_rgb(142, 220, 255)
            };
            ui.label(
                egui::RichText::new(icon_text)
                    .size(24.0)
                    .color(icon_color),
            );

            ui.add_space(4.0);

            ui.label(
                egui::RichText::new(message)
                    .size(12.5)
                    .color(style::TEXT_SECONDARY),
            );

            ui.add_space(12.0);

            // Buttons
            ui.horizontal(|ui| {
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let confirm_color = if is_danger {
                            style::DANGER
                        } else {
                            style::ACCENT
                        };
                        let text_color = if is_danger {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::BLACK
                        };

                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(i18n::ts(locale, confirm_key))
                                        .color(text_color),
                                )
                                .fill(confirm_color),
                            )
                            .clicked()
                        {
                            // We can't move on_confirm, so we match the type
                            *result = match &on_confirm {
                                ModalResult::ConfirmDelete => ModalResult::ConfirmDelete,
                                ModalResult::ConfirmClear => ModalResult::ConfirmClear,
                                ModalResult::ConfirmReindex => ModalResult::ConfirmReindex,
                                _ => ModalResult::None,
                            };
                            *close = true;
                        }

                        if ui
                            .button(i18n::ts(locale, "modal_cancel"))
                            .clicked()
                        {
                            *close = true;
                        }
                    },
                );
            });
        });

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *close = true;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        *result = match &on_confirm {
            ModalResult::ConfirmDelete => ModalResult::ConfirmDelete,
            ModalResult::ConfirmClear => ModalResult::ConfirmClear,
            ModalResult::ConfirmReindex => ModalResult::ConfirmReindex,
            _ => ModalResult::None,
        };
        *close = true;
    }
}
