//! The Root of the Perf UI.
//!
//! This is where the properties for the whole Perf UI are set,
//! and what manages the UI for all your entries.

use bevy::prelude::*;
use egui::Align2;

/// Which corner of the screen to display the Perf UI at?
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerfUiPosition {
    /// Absolute positioning based on distance from top and left edges of viewport.
    TopLeft,
    /// Absolute positioning based on distance from top and right edges of viewport.
    #[default]
    TopRight,
    /// Absolute positioning based on distance from bottom and left edges of viewport.
    BottomLeft,
    /// Absolute positioning based on distance from bottom and right edges of viewport.
    BottomRight,
}

/// Component to configure a Perf UI instance.
///
/// To create a Perf UI, spawn an entity with this component
/// + any components for the entries you want to display:
///
/// ```rust,ignore
/// commands.spawn((
///     PerfUiRoot {
///         // ... settings ...
///         ..default()
///     },
///     PerfUiEntryFPS {
///         // ... settings ...
///         ..default()
///     },
///     /// ...
/// ));
/// ```
#[derive(Component, Debug, Clone)]
pub struct PerfUiRoot {
    /// The color to use for the background of the Perf UI.
    ///
    /// Default: BLACK with alpha 0.5
    pub background_color: Color,
    /// The color to use for the background of each entry/row.
    ///
    /// Default: NONE
    pub inner_background_color: Color,
    /// The color to use for the background of highlighted entries.
    ///
    /// Default: RED with alpha 1/16
    pub inner_background_color_highlight: Color,
    /// Should labels be displayed?
    /// If false, there will be no column for labels, only bare values.
    ///
    /// Default: `true`
    pub display_labels: bool,
    /// Display entries horizontally instead of vertically.
    ///
    /// Default: `false`
    pub layout_horizontal: bool,
    /// The text to display if a value cannot be obtained.
    ///
    /// Default: `"N/A"`
    pub text_err: String,
    /// The color for the error text.
    ///
    /// Default: DARK_GRAY
    pub err_color: Color,
    /// The color to use for entries that do not provide a custom color.
    ///
    /// Default: GRAY
    pub default_value_color: Color,
    /// The color to use for label text.
    ///
    /// Default: WHITE
    pub label_color: Color,
    /// The font size for labels.
    ///
    /// Default: `12.0`
    pub fontsize_label: f32,
    /// The font size for values.
    ///
    /// Default: `12.0`
    pub fontsize_value: f32,
    /// The position of the UI.
    ///
    /// Default: top-right corner
    pub position: PerfUiPosition,
    /// Distance from the edge of the screen in pixels
    ///
    /// Default: `16.0`
    pub margin: f32,
    /// Empty space around the edge of the Perf UI
    ///
    /// Default: `2.0`
    pub padding: f32,
    /// Empty space around entries (rows) in pixels
    ///
    /// Default: `0.0`
    pub inner_margin: f32,
    /// Empty space around the text in every row
    ///
    /// Default: `0.0`
    pub inner_padding: f32,
    /// The width (in pixels) of the values column
    ///
    /// Default: `128.0`
    pub values_col_width: f32,
    /// Z-index for drawing the Perf UI on top of other UI.
    ///
    /// Roots with larger values are drawn later / on top.
    /// Only meaningful relative to other `PerfUiRoot`s of the same
    /// egui context (window).
    ///
    /// Default: `0`
    pub z_index: i32,
    /// The font to use for labels.
    ///
    /// Loaded automatically: once the font asset is loaded, it is registered
    /// into egui's font definitions and used for measuring and rendering
    /// label text.
    /// Until then (or if the asset is not available), egui's default
    /// proportional font is used. Registering/activating the font in egui
    /// takes one additional frame after the asset has been loaded, so the
    /// very first frames may still use the fallback font.
    pub font_label: Handle<Font>,
    /// The font to use for values.
    ///
    /// See [`Self::font_label`] for details on automatic loading/registration.
    pub font_value: Handle<Font>,
    /// The font to use for highlighted values.
    ///
    /// See [`Self::font_label`] for details on automatic loading/registration.
    pub font_highlight: Handle<Font>,
}

impl Default for PerfUiRoot {
    fn default() -> Self {
        PerfUiRoot {
            background_color: Color::srgba(0.0, 0.0, 0.0, 0.5),
            inner_background_color: Color::NONE,
            inner_background_color_highlight: Color::srgba(1.0, 0.0, 0.0, 1.0 / 16.0),
            display_labels: true,
            layout_horizontal: false,
            text_err: "N/A".into(),
            err_color: Color::srgb(0.5, 0.5, 0.5),
            default_value_color: Color::srgb(0.75, 0.75, 0.75),
            label_color: Color::srgb(1.0, 1.0, 1.0),
            fontsize_label: 12.0,
            fontsize_value: 12.0,
            position: default(),
            margin: 16.0,
            padding: 2.0,
            inner_margin: 0.0,
            inner_padding: 0.0,
            values_col_width: 128.0,
            z_index: 0,
            font_label: Handle::default(),
            font_value: Handle::default(),
            font_highlight: Handle::default(),
        }
    }
}

impl PerfUiRoot {
    pub(crate) fn egui_anchor(&self) -> Align2 {
        match self.position {
            PerfUiPosition::TopLeft => Align2::LEFT_TOP,
            PerfUiPosition::TopRight => Align2::RIGHT_TOP,
            PerfUiPosition::BottomLeft => Align2::LEFT_BOTTOM,
            PerfUiPosition::BottomRight => Align2::RIGHT_BOTTOM,
        }
    }
}