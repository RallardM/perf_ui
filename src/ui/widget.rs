//! Framework for creating different widgets for displaying Perf UI entries using egui.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::entry::PerfUiEntry;
use crate::ui::root::PerfUiRoot;
use crate::utils::to_egui_color;

/// Extra empty space around the label text inside the label wrapper (in pixels).
/// Mirrors the padding of the original (bevy_ui-based) implementation.
pub const LABEL_PADDING: f32 = 4.0;
/// Extra empty space around the value inside the values column (in pixels).
/// Mirrors the padding of the original (bevy_ui-based) implementation.
pub const VALUE_PADDING: f32 = 4.0;

/// The registered egui font names corresponding to the custom fonts of a
/// [`PerfUiRoot`].
///
/// When a [`Handle<Font>`] configured in the [`PerfUiRoot`] has been loaded
/// and registered into egui's font definitions (which [`crate::PerfUiPlugin`]
/// does automatically), the corresponding egui font name is used for the text.
/// If a font is not (yet) registered/loaded, the default proportional font is
/// used instead.
#[derive(Debug, Clone, Default)]
pub struct PerfUiRowFonts {
    /// Registered egui font name for `PerfUiRoot::font_label`, if any.
    pub label: Option<String>,
    /// Registered egui font name for `PerfUiRoot::font_value`, if any.
    pub value: Option<String>,
    /// Registered egui font name for `PerfUiRoot::font_highlight`, if any.
    pub highlight: Option<String>,
}

impl PerfUiRowFonts {
    /// The egui [`egui::FontId`] for label text.
    pub fn label_font_id(&self, size: f32) -> egui::FontId {
        match &self.label {
            Some(name) => egui::FontId::new(size, egui::FontFamily::Name(name.clone().into())),
            None => egui::FontId::proportional(size),
        }
    }

    /// The egui [`egui::FontId`] for value text.
    ///
    /// If `highlight` is true and a highlight font is registered, it is used.
    pub fn value_font_id(&self, size: f32, highlight: bool) -> egui::FontId {
        if highlight && let Some(name) = &self.highlight {
            return egui::FontId::new(size, egui::FontFamily::Name(name.clone().into()));
        }
        match &self.value {
            Some(name) => egui::FontId::new(size, egui::FontFamily::Name(name.clone().into())),
            None => egui::FontId::proportional(size),
        }
    }
}

/// Layout context for a single widget row, prepared by the renderer.
///
/// The renderer measures the natural width of every widget row of a Perf UI
/// (using font metrics), so that all rows end up exactly the same width
/// (like in the original, bevy_ui-based implementation), and passes the
/// resulting panel geometry down to the actual drawing code.
pub struct PerfUiRowCtx<'a> {
    /// Total content width for this row (the panel width).
    ///
    /// In horizontal layouts, this equals the row's own natural width.
    pub content_width: f32,
    /// The natural (minimum) width of this row, including the label part
    /// and the (minimum) width of the values column.
    pub natural: f32,
    /// The registered egui fonts to use for the text of this row.
    pub fonts: &'a PerfUiRowFonts,
}

impl PerfUiRowCtx<'_> {
    /// The flex space that should be added before the value cluster,
    /// to make this row reach the full panel width.
    pub fn flex_pad(&self) -> f32 {
        (self.content_width - self.natural).max(0.0)
    }
}

/// Measure the width of a text string, without rendering it.
pub fn measure_text(ui: &egui::Ui, text: &str, font_id: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font_id.clone(), egui::Color32::WHITE)
        .size()
        .x
}

/// Draw one widget row: the row background ("strip"), the label part,
/// and the value cluster (right-aligned within the values column).
///
/// This is the building-block for custom widgets: it reproduces the row
/// geometry of the original (bevy_ui-based) implementation:
/// `[padding][label][padding][flex space][values column]`
///
/// Use this inside [`PerfUiWidget::render`]: the provided `ui` is the
/// parent layout (do not add anything else to it directly).
pub fn perf_ui_row(
    ui: &mut egui::Ui,
    root: &PerfUiRoot,
    row: &PerfUiRowCtx<'_>,
    highlight: bool,
    label: Option<&str>,
    add_value_cluster: &mut dyn FnMut(&mut egui::Ui),
) -> egui::Response {
    let strip = if highlight {
        root.inner_background_color_highlight
    } else {
        root.inner_background_color
    };

    let flex_pad = row.flex_pad();

    egui::Frame::NONE
        .fill(to_egui_color(strip))
        .inner_margin(root.inner_padding)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if root.display_labels && let Some(label) = label {
                    ui.horizontal(|ui| {
                        ui.add_space(LABEL_PADDING);
                        ui.label(
                            egui::RichText::new(format!("{label}:"))
                                .size(root.fontsize_label)
                                .color(to_egui_color(root.label_color))
                                .font(row.fonts.label_font_id(root.fontsize_label)),
                        );
                        ui.add_space(LABEL_PADDING);
                    });
                }
                ui.horizontal(|ui| {
                    ui.add_space(flex_pad);
                    add_value_cluster(ui);
                })
            })
        })
        .response
}

/// Trait for Perf UI widgets using egui.
pub trait PerfUiWidget<E: PerfUiEntry>: Component + Clone + Send + Sync + 'static {
    /// The per-frame data this widget needs to render.
    type Data: Send + Sync + 'static;

    /// The underlying entry used as this widget's data source.
    fn entry(&self) -> &E;

    /// Get the sort key for this widget.
    fn sort_key(&self) -> i32 {
        self.entry().sort_key()
    }

    /// Compute the data to display this frame.
    fn make_data(
        &self,
        root: &PerfUiRoot,
        param: &mut <E::SystemParam as SystemParam>::Item<'_, '_>,
    ) -> Self::Data;

    /// The natural width of this widget's row (including the label part
    /// and the minimum width of the values column).
    ///
    /// The renderer uses this to size the whole panel, so that all rows share
    /// the same width, like in the original (bevy_ui-based) implementation.
    ///
    /// - `fonts`: the registered egui fonts for this Perf UI.
    /// - `cached`: a cached natural width, measured from the rows that were
    ///   actually drawn in the previous frame (if any). Only meaningful for
    ///   widgets whose natural width cannot be computed from font metrics
    ///   alone; implementations that measure exactly (like the built-in text
    ///   rows) must ignore it — a stale cached width would inflate the row's
    ///   natural width and break the right-alignment of the values.
    /// - `measure`: a callback to measure the width of a text string with a
    ///   given egui font.
    ///
    /// The default implementation just returns the cached value (or `0.0`):
    /// custom widgets that use [`perf_ui_row`] can rely on the measured
    /// widths of their actual drawing (the renderer caches the width of every
    /// drawn row, and provides it as `cached` in subsequent frames).
    fn natural_width(
        &self,
        root: &PerfUiRoot,
        fonts: &PerfUiRowFonts,
        data: &Self::Data,
        cached: Option<f32>,
        measure: &mut dyn FnMut(&str, &egui::FontId) -> f32,
    ) -> f32 {
        let _ = (root, fonts, data, measure);
        cached.unwrap_or(0.0)
    }

    /// Render this widget using egui, as one row of its Perf UI panel.
    ///
    /// Returns the natural width of the rendered row (label + value cluster),
    /// excluding the flex padding added to fill the panel width. Widgets
    /// build their row using [`perf_ui_row`], and can simply call
    /// [`row_natural_width`] on the result.
    fn render(
        &self,
        root: &PerfUiRoot,
        ui: &mut egui::Ui,
        row: &PerfUiRowCtx<'_>,
        data: &Self::Data,
    ) -> f32;
}

/// Compute the natural width of a row that was drawn using [`perf_ui_row`],
/// from the [`egui::Response`] of that call.
pub fn row_natural_width(
    response: &egui::Response,
    root: &PerfUiRoot,
    row: &PerfUiRowCtx<'_>,
) -> f32 {
    (response.rect.width() - 2.0 * root.inner_padding - row.flex_pad()).max(0.0)
}

/// Data used by the default/simple text-row widget.
#[derive(Clone, Debug)]
pub struct SimpleRowData {
    /// The formatted value text.
    pub text: String,
    /// The color to render the value text in.
    pub color: Color,
    /// Whether the row should be highlighted.
    pub highlight: bool,
}

impl<E> PerfUiWidget<E> for E
where
    E: PerfUiEntry + Clone + Send + Sync + 'static,
{
    type Data = SimpleRowData;

    fn entry(&self) -> &E {
        self
    }

    fn make_data(
        &self,
        root: &PerfUiRoot,
        param: &mut <E::SystemParam as SystemParam>::Item<'_, '_>,
    ) -> Self::Data {
        if let Some(value) = self.update_value(param) {
            Self::Data {
                text: self.format_value(&value).trim().to_owned(),
                color: self.value_color(&value).unwrap_or(root.default_value_color),
                highlight: self.value_highlight(&value),
            }
        } else {
            Self::Data {
                text: root.text_err.trim().to_owned(),
                color: root.err_color,
                highlight: false,
            }
        }
    }

    fn natural_width(
        &self,
        root: &PerfUiRoot,
        fonts: &PerfUiRowFonts,
        data: &Self::Data,
        _cached: Option<f32>,
        measure: &mut dyn FnMut(&str, &egui::FontId) -> f32,
    ) -> f32 {
        let label_part = if root.display_labels {
            measure(
                &format!("{}:", self.label()),
                &fonts.label_font_id(root.fontsize_label),
            ) + 2.0 * LABEL_PADDING
        } else {
            0.0
        };
        let value_w = measure(
            &data.text,
            &fonts.value_font_id(root.fontsize_value, data.highlight),
        );
        let col_part = (value_w + 2.0 * VALUE_PADDING).max(root.values_col_width);
        // Measured exactly from the font metrics: the cached width is
        // deliberately not used here (see the trait docs).
        label_part + col_part
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

        let response = perf_ui_row(
            ui,
            root,
            row,
            highlight,
            Some(self.label()),
            &mut |ui| {
                let value_w = measure_text(ui, &data.text, &value_font);
                let col_part = (value_w + 2.0 * VALUE_PADDING).max(root.values_col_width);
                ui.horizontal(|ui| {
                    // Right-align the value inside the values column:
                    ui.add_space((col_part - value_w - VALUE_PADDING).max(0.0));
                    ui.label(
                        egui::RichText::new(&data.text)
                            .size(root.fontsize_value)
                            .color(to_egui_color(data.color))
                            .font(value_font.clone()),
                    );
                    ui.add_space(VALUE_PADDING);
                });
            },
        );
        row_natural_width(&response, root, row)
    }
}
