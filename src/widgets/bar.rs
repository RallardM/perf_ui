//! Bar Widget
//!
//! Displays a Perf UI entry as a "bar", instead of a bare value.
//!
//! To use it, simply wrap your entry type in the [`PerfUiWidgetBar`]
//! struct, and insert that as a component to your Perf UI entity,
//! instead of inserting the entry directly as a component.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::entry::{PerfUiEntry, PerfUiEntryDisplayRange};
use crate::ui::root::PerfUiRoot;
use crate::ui::widget::{
    measure_text, perf_ui_row, row_natural_width, PerfUiRowCtx, PerfUiRowFonts, PerfUiWidget,
    LABEL_PADDING, VALUE_PADDING,
};
use crate::utils::{ColorGradient, to_egui_color};

/// Where should the text value be displayed inside the bar?
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BarTextPosition {
    /// Do not display the value as text. Bar only.
    NoText,
    /// Position the text inside the bar, at the center.
    #[default]
    Center,
    /// Position the text inside the bar, at the start.
    Start,
    /// Position the text inside the bar, at the end.
    End,
    /// Position the text outside the bar, at the start.
    OutsideStart,
    /// Position the text outside the bar, at the end.
    OutsideEnd,
}

/// Which way should the bar fill up?
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BarFillDirection {
    /// From left to right.
    #[default]
    Left,
    /// From the center, expanding towards both sides.
    Center,
    /// From right to left.
    Right,
}

/// Display a Perf UI entry as a Bar Widget.
///
/// This struct wraps the entry type, which will be the source
/// of the data value to be displayed by the bar.
///
/// It allows you to customize the properties of the bar.
#[derive(Component, Clone)]
#[require(PerfUiRoot)]
pub struct PerfUiWidgetBar<E: PerfUiEntryDisplayRange> {
    /// Should the bar also display the value as text? Where?
    pub text_position: BarTextPosition,
    /// Set the color of the text that displays the value.
    pub text_color_override: Option<Color>,
    /// Which way should the bar fill up?
    pub fill_direction: BarFillDirection,
    /// What should be the color of the filled portion of the bar?
    pub bar_color: ColorGradient,
    /// What should be the color of the unfilled portion of the bar?
    pub bar_background: Color,
    /// The thickness of the bar's border.
    pub bar_border_px: f32,
    /// The color of the bar's border.
    pub bar_border_color: Color,
    /// Force the bar to have a specific height in pixels.
    pub bar_height_px: Option<f32>,
    /// Force the bar to have a specific length in pixels.
    pub bar_length_px: Option<f32>,
    /// The entry (data source for the bar widget).
    pub entry: E,
}

/// Per-frame data for a bar widget.
#[derive(Clone, Debug)]
pub struct PerfUiWidgetBarData {
    /// The formatted value text.
    pub text: String,
    /// The color of the text.
    pub text_color: Color,
    /// The color of the filled portion of the bar.
    pub fill_color: Color,
    /// How full the bar should appear, between 0.0 and 1.0.
    pub fill_pct: f32,
    /// Whether the row should be highlighted.
    pub highlight: bool,
    /// Whether the value is unavailable.
    pub error: bool,
}

impl<V, E> PerfUiWidgetBar<E>
where
    V: num_traits::Num + num_traits::ToPrimitive + Copy,
    E: PerfUiEntry<Value = V> + PerfUiEntryDisplayRange,
{
    /// Create a new Bar widget with default settings
    pub fn new(entry: E) -> Self {
        Self {
            text_position: default(),
            text_color_override: None,
            fill_direction: default(),
            bar_color: ColorGradient::single(Color::srgb(0.5, 0.5, 0.5)),
            bar_background: Color::srgba(0.0, 0.0, 0.0, 0.5),
            bar_border_color: Color::srgb(0.0, 0.0, 0.0),
            bar_border_px: 1.0,
            bar_height_px: None,
            bar_length_px: None,
            entry,
        }
    }

    fn get_range(&self) -> Option<(f64, f64)> {
        use num_traits::NumCast;

        let g_min = self.bar_color.min_stop().map(|(v, _)| *v as f64);
        let g_max = self.bar_color.max_stop().map(|(v, _)| *v as f64);
        let h_min = self
            .entry
            .min_value_hint()
            .and_then(|v| <f64 as NumCast>::from(v));
        let h_max = self
            .entry
            .max_value_hint()
            .and_then(|v| <f64 as NumCast>::from(v));

        if g_min == g_max {
            if let (Some(h_min), Some(h_max)) = (h_min, h_max) {
                return Some((h_min, h_max));
            } else {
                return None;
            }
        }

        let v_min = match (g_min, h_min) {
            (Some(g_min), Some(h_min)) => g_min.min(h_min),
            (Some(g_min), None) => g_min,
            (None, Some(h_min)) => h_min,
            (None, None) => return None,
        };

        let v_max = match (g_max, h_max) {
            (Some(g_max), Some(h_max)) => g_max.max(h_max),
            (Some(g_max), None) => g_max,
            (None, Some(h_max)) => h_max,
            (None, None) => return None,
        };

        Some((v_min, v_max))
    }
}

impl<V, E> PerfUiWidget<E> for PerfUiWidgetBar<E>
where
    V: num_traits::Num + num_traits::ToPrimitive + Copy,
    E: PerfUiEntry<Value = V> + PerfUiEntryDisplayRange + Clone + Send + Sync + 'static,
{
    type Data = PerfUiWidgetBarData;

    fn entry(&self) -> &E {
        &self.entry
    }

    fn make_data(
        &self,
        root: &PerfUiRoot,
        param: &mut <E::SystemParam as SystemParam>::Item<'_, '_>,
    ) -> Self::Data {
        use num_traits::NumCast;

        let value = self.entry.update_value(param);
        let highlight = value
            .map(|v| self.entry.value_highlight(&v))
            .unwrap_or(false);

        if let Some(value) = value {
            let value_f64 = <f64 as NumCast>::from(value);
            let fill_pct = match (value_f64, self.get_range()) {
                (Some(value), Some((v_min, v_max))) if v_max > v_min => {
                    ((value - v_min) / (v_max - v_min)).clamp(0.0, 1.0) as f32
                }
                _ => 0.0,
            };

            let fill_color = value_f64
                .and_then(|value| self.bar_color.get_color_for_value(value as f32))
                .unwrap_or(self.bar_background);

            Self::Data {
                text: self.entry.format_value(&value).trim().to_owned(),
                text_color: self
                    .text_color_override
                    .or_else(|| self.entry.value_color(&value))
                    .unwrap_or(root.default_value_color),
                fill_color,
                fill_pct,
                highlight,
                error: false,
            }
        } else {
            Self::Data {
                text: root.text_err.trim().to_owned(),
                text_color: self.text_color_override.unwrap_or(root.err_color),
                fill_color: self.bar_background,
                fill_pct: 0.0,
                highlight,
                error: true,
            }
        }
    }

    fn natural_width(
        &self,
        root: &PerfUiRoot,
        fonts: &PerfUiRowFonts,
        data: &Self::Data,
        cached: Option<f32>,
        measure: &mut dyn FnMut(&str, &egui::FontId) -> f32,
    ) -> f32 {
        let label_part = if root.display_labels {
            measure(
                &format!("{}:", self.entry.label()),
                &fonts.label_font_id(root.fontsize_label),
            ) + 2.0 * LABEL_PADDING
        } else {
            0.0
        };
        let bar_w = self.bar_width(root);
        let outside_text_w = if self.text_position_is_outside() {
            measure(
                &data.text,
                &fonts.value_font_id(root.fontsize_value, data.highlight),
            ) + LABEL_PADDING
        } else {
            0.0
        };
        let col_part = (bar_w + outside_text_w + 2.0 * VALUE_PADDING).max(root.values_col_width);
        (label_part + col_part).max(cached.unwrap_or(0.0))
    }

    fn render(
        &self,
        root: &PerfUiRoot,
        ui: &mut egui::Ui,
        row: &PerfUiRowCtx<'_>,
        data: &Self::Data,
    ) -> f32 {
        let highlight = data.highlight;
        let value_font = row.fonts.value_font_id(root.fontsize_value, highlight);
        let bar_w = self.bar_width(root);

        let response = perf_ui_row(
            ui,
            root,
            row,
            highlight,
            Some(self.entry.label()),
            &mut |ui| {
                let outside_text_w = if self.text_position_is_outside() {
                    measure_text(ui, &data.text, &value_font) + LABEL_PADDING
                } else {
                    0.0
                };
                let col_part =
                    (bar_w + outside_text_w + 2.0 * VALUE_PADDING).max(root.values_col_width);

                ui.horizontal(|ui| {
                    // Align the bar (and its outside text) inside the values column
                    ui.add_space((col_part - bar_w - outside_text_w - VALUE_PADDING).max(0.0));

                    let text_color = to_egui_color(data.text_color);

                    if self.text_position == BarTextPosition::OutsideStart {
                        ui.label(
                            egui::RichText::new(&data.text)
                                .size(root.fontsize_value)
                                .color(text_color)
                                .font(value_font.clone()),
                        );
                        ui.add_space(LABEL_PADDING);
                    }

                    let overlay_text = match self.text_position {
                        BarTextPosition::Center
                        | BarTextPosition::Start
                        | BarTextPosition::End => Some(data.text.as_str()),
                        _ => None,
                    };

                    self.render_bar(root, ui, row.fonts, data, bar_w, overlay_text);

                    if self.text_position == BarTextPosition::OutsideEnd {
                        ui.add_space(LABEL_PADDING);
                        ui.label(
                            egui::RichText::new(&data.text)
                                .size(root.fontsize_value)
                                .color(text_color)
                                .font(value_font.clone()),
                        );
                    }

                    ui.add_space(VALUE_PADDING);
                });
            },
        );
        row_natural_width(&response, root, row)
    }
}

impl<V, E> PerfUiWidgetBar<E>
where
    V: num_traits::Num + num_traits::ToPrimitive + Copy,
    E: PerfUiEntry<Value = V> + PerfUiEntryDisplayRange,
{
    /// Whether the value text is displayed outside of the bar.
    pub fn text_position_is_outside(&self) -> bool {
        matches!(
            self.text_position,
            BarTextPosition::OutsideStart | BarTextPosition::OutsideEnd
        )
    }

    /// The length of the bar itself (not counting the outside text, if any).
    pub fn bar_width(&self, root: &PerfUiRoot) -> f32 {
        self.bar_length_px
            .unwrap_or(root.values_col_width - 2.0 * VALUE_PADDING)
    }

    fn render_bar(
        &self,
        root: &PerfUiRoot,
        ui: &mut egui::Ui,
        fonts: &PerfUiRowFonts,
        data: &PerfUiWidgetBarData,
        bar_width: f32,
        overlay_text: Option<&str>,
    ) {
        let height = self
            .bar_height_px
            .unwrap_or(root.fontsize_value * 1.6)
            .max(6.0);

        let (rect, _response) = ui.allocate_exact_size(
            egui::vec2(bar_width, height),
            egui::Sense::hover(),
        );
        let painter = ui.painter();

        painter.rect_filled(rect, 0.0, to_egui_color(self.bar_background));

        let inner = if self.bar_border_px > 0.0 {
            rect.shrink(self.bar_border_px)
        } else {
            rect
        };

        let pct = data.fill_pct.clamp(0.0, 1.0);
        let inner_width = inner.width().max(0.0);
        let fill_rect = match self.fill_direction {
            BarFillDirection::Left => {
                egui::Rect::from_min_max(inner.min, egui::pos2(inner.min.x + inner_width * pct, inner.max.y))
            }
            BarFillDirection::Right => {
                egui::Rect::from_min_max(egui::pos2(inner.max.x - inner_width * pct, inner.min.y), inner.max)
            }
            BarFillDirection::Center => {
                let center = inner.center().x;
                let half = inner_width * pct * 0.5;
                egui::Rect::from_min_max(
                    egui::pos2(center - half, inner.min.y),
                    egui::pos2(center + half, inner.max.y),
                )
            }
        };

        if pct > 0.0 {
            painter.rect_filled(fill_rect, 0.0, to_egui_color(data.fill_color));
        }

        if self.bar_border_px > 0.0 {
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(self.bar_border_px, to_egui_color(self.bar_border_color)),
                egui::StrokeKind::Inside,
            );
        }

        if let Some(text) = overlay_text {
            let (pos, anchor) = match self.text_position {
                BarTextPosition::Start => (
                    egui::pos2(inner.min.x + 2.0, inner.center().y),
                    egui::Align2::LEFT_CENTER,
                ),
                BarTextPosition::End => (
                    egui::pos2(inner.max.x - 2.0, inner.center().y),
                    egui::Align2::RIGHT_CENTER,
                ),
                _ => (inner.center(), egui::Align2::CENTER_CENTER),
            };

            painter.text(
                pos,
                anchor,
                text,
                fonts.value_font_id(root.fontsize_value, data.highlight),
                to_egui_color(data.text_color),
            );
        }
    }
}