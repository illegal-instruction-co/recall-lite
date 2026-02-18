use eframe::egui;

use crate::i18n::{self, Language};
use crate::state::SearchResult;

use super::style;

pub enum ResultAction {
    None,
    Select(usize),
    Open(usize),
}

fn get_file_icon(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "pdf" | "txt" | "md" => "\u{1F4C4}",         // document
        "rs" | "ts" | "js" | "py" | "go" | "java" | "c" | "cpp" | "cs" => "\u{1F4BB}", // code
        "json" | "yaml" | "yml" | "toml" => "\u{2699}", // config gear
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => "\u{1F5BC}", // image
        _ => "\u{1F4C1}",                              // file
    }
}

fn get_filename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

pub fn show(
    ui: &mut egui::Ui,
    results: &[SearchResult],
    selected_index: usize,
    active_container: &str,
    query: &str,
    locale: Language,
) -> ResultAction {
    let mut action = ResultAction::None;

    // Empty states
    if results.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);

            if query.is_empty() {
                // No query - show container info
                ui.label(
                    egui::RichText::new("\u{25A0}")
                        .size(32.0)
                        .color(style::ACCENT.linear_multiply(0.4)),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(active_container)
                        .size(16.0)
                        .color(style::TEXT_PRIMARY)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(i18n::ts(locale, "results_container_active"))
                        .size(12.0)
                        .color(style::TEXT_SECONDARY),
                );
                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new(i18n::ts(locale, "results_shortcuts").to_uppercase())
                        .size(10.0)
                        .color(style::TEXT_TERTIARY),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() * 0.25);
                    ui.label(
                        egui::RichText::new(i18n::ts(locale, "results_shortcut_index"))
                            .size(11.0)
                            .color(style::TEXT_DISABLED)
                            .monospace(),
                    );
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new(i18n::ts(locale, "results_shortcut_toggle"))
                            .size(11.0)
                            .color(style::TEXT_DISABLED)
                            .monospace(),
                    );
                });
            } else {
                // Query but no results
                ui.label(
                    egui::RichText::new(i18n::ts(locale, "results_no_results"))
                        .size(14.0)
                        .color(style::TEXT_PRIMARY),
                );
                ui.label(
                    egui::RichText::new(i18n::t(
                        locale,
                        "results_in_container",
                        &[("container", active_container)],
                    ))
                    .size(12.0)
                    .color(style::TEXT_SECONDARY),
                );
            }
        });
        return action;
    }

    // Results scroll area
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            for (idx, result) in results.iter().enumerate() {
                let is_selected = idx == selected_index;

                let bg = if is_selected {
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 8)
                } else {
                    egui::Color32::TRANSPARENT
                };

                let frame = egui::Frame::new()
                    .fill(bg)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 8 })
                    .stroke(if is_selected {
                        egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 13),
                        )
                    } else {
                        egui::Stroke::NONE
                    });

                let response = frame
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        // Accent pill for selected item
                        if is_selected {
                            let rect = ui.cursor();
                            let pill_rect = egui::Rect::from_min_size(
                                egui::pos2(rect.left() - 8.0, rect.top()),
                                egui::vec2(3.0, 16.0),
                            );
                            ui.painter().rect_filled(pill_rect, 1.5, style::ACCENT);
                        }

                        ui.horizontal(|ui| {
                            // File icon
                            ui.label(
                                egui::RichText::new(get_file_icon(&result.path))
                                    .size(14.0)
                                    .color(style::TEXT_SECONDARY),
                            );

                            ui.vertical(|ui| {
                                // Filename + score
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(get_filename(&result.path))
                                            .size(13.0)
                                            .color(style::TEXT_PRIMARY)
                                            .strong(),
                                    );

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let score_text =
                                                format!("{}%", result.score.round() as i32);
                                            let color = style::score_color(result.score);
                                            ui.label(
                                                egui::RichText::new(score_text)
                                                    .size(10.0)
                                                    .color(color),
                                            );
                                        },
                                    );
                                });

                                // Snippet
                                if !result.snippet.is_empty() {
                                    let snippet_display: String =
                                        result.snippet.chars().take(120).collect();
                                    ui.label(
                                        egui::RichText::new(snippet_display)
                                            .size(11.0)
                                            .color(style::TEXT_SECONDARY),
                                    );
                                }

                                // Full path
                                ui.label(
                                    egui::RichText::new(&result.path)
                                        .size(10.0)
                                        .color(style::TEXT_DISABLED)
                                        .monospace(),
                                );
                            });
                        });
                    })
                    .response;

                let response = response.interact(egui::Sense::click());
                if response.clicked() {
                    action = ResultAction::Open(idx);
                } else if response.hovered() {
                    if !is_selected {
                        action = ResultAction::Select(idx);
                    }
                }

                // Ensure selected item is visible
                if is_selected {
                    response.scroll_to_me(Some(egui::Align::Center));
                }
            }
        });

    action
}
