use eframe::egui;

use crate::i18n::{self, Language};
use crate::state::ContainerListItem;

use super::style;

pub enum SidebarAction {
    None,
    ToggleSidebar,
    SwitchContainer(String),
    CreateContainer,
    DeleteContainer,
    ClearIndex,
    ReindexAll,
    CycleLocale,
}

pub fn show(
    ui: &mut egui::Ui,
    containers: &[ContainerListItem],
    active_container: &str,
    sidebar_open: bool,
    is_indexing: bool,
    locale: Language,
) -> SidebarAction {
    let mut action = SidebarAction::None;

    let sidebar_width = if sidebar_open { 200.0 } else { 48.0 };

    ui.allocate_ui_with_layout(
        egui::vec2(sidebar_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            ui.set_min_width(sidebar_width);
            ui.set_max_width(sidebar_width);

            let bg = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 51);
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, bg);

            ui.add_space(8.0);

            // Header
            ui.horizontal(|ui| {
                let toggle_text = if sidebar_open { "\u{25C0}" } else { "\u{25B6}" };
                let tooltip = if sidebar_open {
                    i18n::ts(locale, "sidebar_collapse")
                } else {
                    i18n::ts(locale, "sidebar_expand")
                };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(toggle_text)
                                .size(10.0)
                                .color(style::TEXT_SECONDARY),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .frame(false),
                    )
                    .on_hover_text(tooltip)
                    .clicked()
                {
                    action = SidebarAction::ToggleSidebar;
                }

                if sidebar_open {
                    ui.label(
                        egui::RichText::new(i18n::ts(locale, "sidebar_title").to_uppercase())
                            .size(11.0)
                            .color(style::TEXT_SECONDARY)
                            .strong(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("+")
                                        .size(14.0)
                                        .color(style::TEXT_SECONDARY),
                                )
                                .fill(egui::Color32::TRANSPARENT)
                                .frame(false),
                            )
                            .on_hover_text(i18n::ts(locale, "sidebar_create"))
                            .clicked()
                        {
                            action = SidebarAction::CreateContainer;
                        }
                    });
                }
            });

            if sidebar_open {
                ui.add(egui::Separator::default());

                // Container list
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for container in containers {
                            let is_active = container.name == active_container;

                            let bg_color = if is_active {
                                style::FILL_CONTROL_HOVER
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            let text_color = if is_active {
                                style::TEXT_PRIMARY
                            } else {
                                style::TEXT_SECONDARY
                            };

                            let frame = egui::Frame::new()
                                .fill(bg_color)
                                .corner_radius(6.0)
                                .inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 8 })
                                .stroke(if is_active {
                                    egui::Stroke::new(1.0, style::STROKE_SUBTLE)
                                } else {
                                    egui::Stroke::NONE
                                });

                            let response = frame
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        let icon_color = if is_active {
                                            style::ACCENT
                                        } else {
                                            style::TEXT_TERTIARY
                                        };
                                        ui.label(
                                            egui::RichText::new("\u{25A0}")
                                                .size(12.0)
                                                .color(icon_color),
                                        );
                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new(&container.name)
                                                    .size(13.0)
                                                    .color(text_color),
                                            );
                                            if !container.description.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(&container.description)
                                                        .size(10.0)
                                                        .color(style::TEXT_DISABLED),
                                                );
                                            }
                                        });
                                    });
                                })
                                .response;

                            if response.interact(egui::Sense::click()).clicked() {
                                action =
                                    SidebarAction::SwitchContainer(container.name.clone());
                            }

                            // Show indexed paths for active container
                            if is_active {
                                ui.indent("indexed_paths", |ui| {
                                    ui.label(
                                        egui::RichText::new(
                                            i18n::ts(locale, "sidebar_indexed_folders")
                                                .to_uppercase(),
                                        )
                                        .size(9.0)
                                        .color(style::TEXT_TERTIARY),
                                    );

                                    if container.indexed_paths.is_empty() {
                                        ui.label(
                                            egui::RichText::new(i18n::ts(
                                                locale,
                                                "sidebar_no_folders",
                                            ))
                                            .size(10.0)
                                            .color(style::TEXT_DISABLED)
                                            .italics(),
                                        );
                                    } else {
                                        for path in &container.indexed_paths {
                                            let short: String = path
                                                .rsplit(['/', '\\'])
                                                .take(2)
                                                .collect::<Vec<_>>()
                                                .into_iter()
                                                .rev()
                                                .collect::<Vec<_>>()
                                                .join("/");
                                            ui.label(
                                                egui::RichText::new(format!("\u{1F4C2} {}", short))
                                                    .size(10.0)
                                                    .color(style::TEXT_SECONDARY),
                                            )
                                            .on_hover_text(path);
                                        }

                                        // Rebuild button
                                        let rebuild_btn = ui.add_enabled(
                                            !is_indexing,
                                            egui::Button::new(
                                                egui::RichText::new(format!(
                                                    "\u{21BB} {}",
                                                    i18n::ts(locale, "sidebar_rebuild")
                                                ))
                                                .size(10.0)
                                                .color(style::TEXT_TERTIARY),
                                            )
                                            .fill(egui::Color32::TRANSPARENT),
                                        );
                                        if rebuild_btn
                                            .on_hover_text(i18n::ts(
                                                locale,
                                                "sidebar_rebuild_tooltip",
                                            ))
                                            .clicked()
                                        {
                                            action = SidebarAction::ReindexAll;
                                        }

                                        // Clear Index button
                                        let clear_btn = ui.add_enabled(
                                            !is_indexing,
                                            egui::Button::new(
                                                egui::RichText::new(format!(
                                                    "\u{1F5D1} {}",
                                                    i18n::ts(locale, "sidebar_clear")
                                                ))
                                                .size(10.0)
                                                .color(style::DANGER),
                                            )
                                            .fill(egui::Color32::TRANSPARENT),
                                        );
                                        if clear_btn
                                            .on_hover_text(i18n::ts(
                                                locale,
                                                "sidebar_clear_tooltip",
                                            ))
                                            .clicked()
                                        {
                                            action = SidebarAction::ClearIndex;
                                        }
                                    }
                                });
                            }

                            ui.add_space(2.0);
                        }
                    });

                // Delete container button
                if active_container != "Default" {
                    ui.add_space(4.0);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "\u{1F5D1} {}",
                                    i18n::ts(locale, "sidebar_delete")
                                ))
                                .size(11.0)
                                .color(style::DANGER),
                            )
                            .fill(egui::Color32::TRANSPARENT),
                        )
                        .clicked()
                    {
                        action = SidebarAction::DeleteContainer;
                    }
                }

                // Locale switcher at bottom
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "\u{1F310} {}",
                                    locale.code().to_uppercase()
                                ))
                                .size(11.0)
                                .color(style::TEXT_TERTIARY),
                            )
                            .fill(egui::Color32::TRANSPARENT),
                        )
                        .on_hover_text(locale.label())
                        .clicked()
                    {
                        action = SidebarAction::CycleLocale;
                    }
                });
            }
        },
    );

    action
}
