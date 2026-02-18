use eframe::egui;

use crate::i18n::{self, Language};

use super::style;

pub fn show(
    ui: &mut egui::Ui,
    query: &mut String,
    active_container: &str,
    _is_indexing: bool,
    locale: Language,
    request_focus: bool,
) {
    ui.add_space(8.0);

    let placeholder = i18n::t(
        locale,
        "search_placeholder",
        &[("container", active_container)],
    );

    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 8))
        .corner_radius(8.0)
        .inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 12 })
        .stroke(egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 13),
        ));

    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            // Search icon
            ui.label(
                egui::RichText::new("\u{1F50D}")
                    .size(14.0)
                    .color(style::TEXT_TERTIARY),
            );

            // Search input - take all available width
            let response = ui.add_sized(
                egui::vec2(ui.available_width(), 20.0),
                egui::TextEdit::singleline(query)
                    .hint_text(
                        egui::RichText::new(placeholder)
                            .color(style::TEXT_DISABLED)
                            .size(15.0),
                    )
                    .font(egui::TextStyle::Body)
                    .text_color(style::TEXT_PRIMARY)
                    .frame(false)
                    .desired_width(f32::INFINITY),
            );

            // Focus uniquement quand aucune modale n'est ouverte
            if request_focus {
                response.request_focus();
            }
        });
    });

    ui.add_space(4.0);
}
