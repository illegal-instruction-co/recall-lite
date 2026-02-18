use eframe::egui;

use crate::i18n::{self, Language};
use crate::state::IndexingProgress;

use super::style;

pub fn show(
    ui: &mut egui::Ui,
    status: &str,
    is_indexing: bool,
    index_progress: Option<&IndexingProgress>,
    active_container: &str,
    folder_count: usize,
    result_count: usize,
    locale: Language,
) {
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        let frame = egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 20, 153))
            .inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 0 });

        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Progress bar
            if is_indexing {
                if let Some(progress) = index_progress {
                    if progress.total > 0 {
                        let pct = progress.current as f32 / progress.total as f32;
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 2.0),
                            egui::Sense::hover(),
                        );
                        // Track
                        ui.painter().rect_filled(
                            rect,
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 15),
                        );
                        // Fill
                        let fill_rect = egui::Rect::from_min_size(
                            rect.min,
                            egui::vec2(rect.width() * pct, rect.height()),
                        );
                        ui.painter().rect_filled(fill_rect, 1.0, style::ACCENT);
                    }
                }
            }

            // Status line
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    // Container name
                    ui.label(
                        egui::RichText::new(active_container)
                            .size(11.0)
                            .color(style::ACCENT)
                            .strong(),
                    );

                    // Divider
                    ui.label(
                        egui::RichText::new("\u{2502}")
                            .size(11.0)
                            .color(style::STROKE_SUBTLE),
                    );

                    // Status text or folder count
                    if !status.is_empty() {
                        if is_indexing {
                            ui.label(
                                egui::RichText::new("\u{23F3}")
                                    .size(10.0)
                                    .color(style::TEXT_TERTIARY),
                            );
                        }
                        let pct_prefix = if let Some(progress) = index_progress {
                            if progress.total > 0 {
                                let pct =
                                    (progress.current as f32 / progress.total as f32 * 100.0)
                                        as i32;
                                format!("{}% \u{00B7} ", pct)
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        };
                        ui.label(
                            egui::RichText::new(format!("{}{}", pct_prefix, status))
                                .size(11.0)
                                .color(style::TEXT_TERTIARY),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(i18n::t(
                                locale,
                                "status_indexed_folders",
                                &[("count", &folder_count.to_string())],
                            ))
                            .size(11.0)
                            .color(style::TEXT_TERTIARY),
                        );

                        if result_count > 0 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "\u{00B7} {}",
                                    i18n::t(locale, "status_result_count", &[("count", &result_count.to_string())])
                                ))
                                .size(11.0)
                                .color(style::TEXT_TERTIARY),
                            );
                        }
                    }

                    // Right-aligned keyboard shortcuts
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "\u{23CE} {}",
                                    i18n::ts(locale, "results_open")
                                ))
                                .size(10.0)
                                .color(style::TEXT_DISABLED)
                                .monospace(),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "\u{2191}\u{2193} {}",
                                    i18n::ts(locale, "results_navigate")
                                ))
                                .size(10.0)
                                .color(style::TEXT_DISABLED)
                                .monospace(),
                            );
                        },
                    );
                },
            );
        });
    });
}
