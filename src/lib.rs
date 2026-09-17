//! Customizable Performance/Debug Overlay for egui
//!
//! This crate provides an implementation of an in-game performance/debug UI overlay
//! for the [Bevy game engine](https://bevyengine.org), rendered with
//! [egui](https://github.com/emilk/egui) via [bevy_egui](https://github.com/vladbat00/bevy_egui).
//!
//! The goal of this crate is to make it as useful as possible for any Bevy project:
//!  - Made with egui (not Bevy UI)
//!  - Easy to set up (see the `simple` example)
//!  - Modular! You decide what info you want to display!
//!    - Choose any combination of predefined entries
//!      (see the `specific_entries` example):
//!      - Framerate (FPS), Frame Time, Frame Count, ECS Entity Count, CPU Usage, RAM Usage,
//!        Render CPU Time, Render GPU Time,
//!        Wall Clock, Running Time, Fixed Time Step, Fixed Overstep,
//!        Cursor Position, Window Resolution, Window Scale Factor, Window Mode, Present Mode
//!    - Implement your own custom entries to display anything you like!
//!      - (see the `custom_minimal` and `custom` examples)
//!  - Customizable appearance/styling (see the `settings`, `fps_minimalist` examples)
//!  - Support for highlighting values using a custom color!
//!    - Allows you to quickly notice if something demands your attention.
//!
//! ---
//!
//! First, make sure to add the plugin to your app:
//!
//! ```rust,ignore
//! app.add_plugins(PerfUiPlugin);
//! ```
//!
//! And then, spawning a Perf UI can be as simple as:
//!
//! ```rust,ignore
//! commands.spawn(PerfUiAllEntries::default());
//! ```
//!
//! If you want to create a Perf UI with specific entries of your choice,
//! just spawn an entity with your desired entries, instead
//! of using this bundle.
//!
//! ```rust,ignore
//! commands.spawn((
//!     PerfUiEntryFPS::default(),
//!     PerfUiEntryClock::default(),
//!     // ...
//! ));
//! ```
//!
//! If you want to customize the appearance, set the various fields in each of the
//! structs, instead of using `default()`. To customize settings that apply to all
//! entries, add the [`crate::ui::root::PerfUiRoot`] component (all predefined
//! entries attach it automatically via `#[require]`).
//!
//! If you want to implement your own custom entry, create a component type
//! to represent your entry (you can use it to store any settings),
//! implement [`crate::entry::PerfUiEntry`] on it, and register it with
//! `app.add_perf_ui_simple_entry::<T>()`.
//!
//! ---
//!
//! Each frame, in the [`Update`](bevy::prelude::Update) schedule, the
//! [`PerfUiSet::Update`] systems compute the per-frame data of every widget
//! and prepare its drawing as a callback in a [`PerfUiRenderData`] resource.
//! The renderer then runs inside [`bevy_egui::EguiPrimaryContextPass`] and
//! draws one egui area per [`PerfUiRoot`].

#![warn(missing_docs)]
#![allow(clippy::type_complexity)]
#![allow(clippy::collapsible_else_if)]

use std::borrow::Cow;
use std::sync::Arc;

use bevy::ecs::change_detection::Tick;
use bevy::ecs::query::FilteredAccessSet;
use bevy::ecs::system::{SystemMeta, SystemParam, SystemParamValidationError};
use bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

use crate::entry::PerfUiEntry;
use crate::ui::root::{PerfUiPosition, PerfUiRoot};
use crate::ui::widget::{PerfUiRowCtx, PerfUiRowFonts, PerfUiWidget};
use crate::utils::to_egui_color;

pub mod entry;
pub mod ui;
pub mod utils;

#[cfg(feature = "entries")]
pub mod entries;
#[cfg(feature = "widgets")]
pub mod widgets;

/// Prelude of common types for users of the library
pub mod prelude {
    #[cfg(feature = "entries")]
    pub use crate::entries::prelude::*;
    pub use crate::ui::root::{PerfUiPosition, PerfUiRoot};
    pub use crate::utils::ColorGradient;
    #[cfg(feature = "widgets")]
    pub use crate::widgets::prelude::*;
    pub use crate::{PerfUiAppExt, PerfUiPlugin};
}

/// The Bevy Plugin
#[derive(Default)]
pub struct PerfUiPlugin;

impl Plugin for PerfUiPlugin {
    fn build(&self, app: &mut App) {
        // Add egui support (if the user hasn't already added it)
        if !app.is_plugin_added::<bevy_egui::EguiPlugin>() {
            app.add_plugins(bevy_egui::EguiPlugin::default());
        }

        app.init_resource::<PerfUiRenderData>();

        app.configure_sets(Update, (PerfUiSet::Setup, PerfUiSet::Update));
        app.add_systems(
            Update,
            clear_perf_ui_data.before(PerfUiSet::Update),
        );

        // The actual egui rendering happens within the schedule provided by bevy_egui
        app.add_systems(EguiPrimaryContextPass, render_perf_ui);

        #[cfg(feature = "entries")]
        app.add_plugins(entries::predefined_entries_plugin);
        #[cfg(all(feature = "entries", feature = "widgets"))]
        app.add_plugins(widgets::predefined_widgets_plugin);
    }
}

/// Extension trait for adding new types of Perf UI Entries and Widgets.
pub trait PerfUiAppExt {
    /// Add support for a custom Perf UI Widget type (component).
    ///
    /// Widgets are paired to entry types. This method adds support
    /// for displaying a specific entry type using a specific widget.
    fn add_perf_ui_widget<W, E>(&mut self) -> &mut Self
    where
        E: PerfUiEntry,
        W: PerfUiWidget<E>;

    /// Add support for a custom Perf UI Entry type (component).
    ///
    /// This adds support for displaying the provided entry type
    /// using the builtin "simple" widget, which just shows the
    /// label string and the current value.
    ///
    /// If you want to display your data in other ways, consider
    /// also calling `add_perf_ui_widget` to add support for displaying
    /// your entry using different UI widgets.
    fn add_perf_ui_simple_entry<T: PerfUiEntry + Clone>(&mut self) -> &mut Self {
        self.add_perf_ui_widget::<T, T>()
    }
}

impl PerfUiAppExt for App {
    fn add_perf_ui_widget<W, E>(&mut self) -> &mut Self
    where
        E: PerfUiEntry,
        W: PerfUiWidget<E>,
    {
        self.add_systems(
            Update,
            update_perf_ui_widget::<E, W>
                .run_if(any_with_component::<W>)
                .in_set(PerfUiSet::Update),
        )
    }
}

/// System Sets to allow you to order things relative to our systems.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerfUiSet {
    /// Anchor set (kept in the `Update` schedule for backwards compatibility).
    ///
    /// If your code spawns or despawns Perf UI entities, you can order
    /// it `.before(PerfUiSet::Setup)` (in the `Update` schedule) to ensure
    /// this crate processes any changes in the same frame.
    Setup,
    /// Systems that compute the per-frame data of Perf UI widgets
    /// (running in the `Update` schedule).
    Update,
}

/// Resource that holds the prepared drawing instructions for the current frame.
///
/// Populated by the [`PerfUiSet::Update`] systems (in `Update`), and consumed
/// by the renderer (in [`bevy_egui::EguiPrimaryContextPass`]).
#[derive(Resource, Default)]
pub(crate) struct PerfUiRenderData {
    entries: Vec<PreparedWidget>,
}

/// A single widget, prepared for rendering with egui.
struct PreparedWidget {
    /// The entity holding the [`PerfUiRoot`] this widget belongs to.
    root_entity: Entity,
    /// The sort key of the widget (see [`PerfUiWidget::sort_key`]).
    sort: i32,
    /// The natural width of this widget's row, measured by the renderer
    /// in the pre-pass each frame (before any drawing happens).
    natural_width: f32,
    /// Measure this widget's natural row width (see
    /// [`PerfUiWidget::natural_width`]).
    measure: Box<
        dyn Fn(
                &egui::Context,
                &PerfUiRowFonts,
                Option<f32>,
                &PerfUiRoot,
            ) -> f32
            + Send
            + Sync,
    >,
    /// Draw this widget's row; returns the natural width of the drawn row.
    draw: Box<dyn Fn(&PerfUiRoot, &mut egui::Ui, &PerfUiRowCtx<'_>) -> f32 + Send + Sync>,
}

/// Resource mapping the custom fonts of [`PerfUiRoot`]s (given as
/// [`Handle<Font>`]) to the font names they have been registered under in
/// egui's font definitions.

/// Wrapper [`SystemParam`](bevy::ecs::system::SystemParam) that provides access
/// to the custom system params item of an entry type.
pub(crate) struct EntryUpdater<'w, 's, E: PerfUiEntry> {
    params: <E::SystemParam as SystemParam>::Item<'w, 's>,
}

// SAFETY: `EntryUpdater` delegates all world access to `E::SystemParam`, which
// is constrained (via `PerfUiEntry`) to be a valid `SystemParam`. The state and
// access registration are forwarded verbatim, so no additional world access is
// made beyond what `E::SystemParam` already declares.
unsafe impl<'w, 's, E: PerfUiEntry> SystemParam for EntryUpdater<'w, 's, E> {
    type State = <E::SystemParam as SystemParam>::State;
    type Item<'w2, 's2> = EntryUpdater<'w2, 's2, E>;

    fn init_state(world: &mut World) -> Self::State {
        <E::SystemParam as SystemParam>::init_state(world)
    }

    fn init_access(
        state: &Self::State,
        system_meta: &mut SystemMeta,
        component_access_set: &mut FilteredAccessSet,
        world: &mut World,
    ) {
        <E::SystemParam as SystemParam>::init_access(
            state,
            system_meta,
            component_access_set,
            world,
        );
    }

    unsafe fn get_param<'w2, 's2>(
        state: &'s2 mut Self::State,
        system_meta: &SystemMeta,
        world: UnsafeWorldCell<'w2>,
        change_tick: Tick,
    ) -> Result<Self::Item<'w2, 's2>, SystemParamValidationError> {
        Ok(EntryUpdater {
            // SAFETY: caller guarantees are forwarded to the delegated `get_param`
            params: unsafe {
                <E::SystemParam as SystemParam>::get_param(state, system_meta, world, change_tick)
            }?,
        })
    }
}

/// Clears the prepared widget data at the beginning of each frame,
/// before the data collection systems run.
fn clear_perf_ui_data(mut render_data: ResMut<PerfUiRenderData>) {
    render_data.entries.clear();
}

/// Data collection system, registered once for each (Entry, Widget) type pair
/// via [`PerfUiAppExt::add_perf_ui_widget`].
///
/// Computes the per-frame data of every matching widget, and prepares its egui
/// measurement/drawing closures in [`PerfUiRenderData`].
pub(crate) fn update_perf_ui_widget<E: PerfUiEntry, W: PerfUiWidget<E>>(
    mut render_data: ResMut<PerfUiRenderData>,
    q_widgets: Query<(Entity, &W, Option<&PerfUiRoot>, Option<&ChildOf>)>,
    q_roots: Query<&PerfUiRoot>,
    q_parents: Query<&ChildOf>,
    mut updater: EntryUpdater<'_, '_, E>,
) {
    for (e_widget, widget, self_root, parent) in &q_widgets {
        // Resolve the Perf UI root for this widget:
        // the widget's own entity, or one of its ancestors.
        let resolved: Option<(Entity, Cow<'_, PerfUiRoot>)> = if let Some(root) = self_root {
            Some((e_widget, Cow::Borrowed(root)))
        } else {
            let mut found = None;
            let mut ancestor = parent.map(|p| p.parent());
            while let Some(ancestor_entity) = ancestor {
                if let Ok(root) = q_roots.get(ancestor_entity) {
                    found = Some((ancestor_entity, Cow::Borrowed(root)));
                    break;
                }
                // Move further up the hierarchy
                ancestor = q_parents.get(ancestor_entity).map(|p| p.parent()).ok();
            }
            found
        };

        let Some((root_entity, root)) = resolved else {
            // Widget with no PerfUiRoot anywhere in its hierarchy: nothing to do.
            // (All predefined entries use `#[require(PerfUiRoot)]`, so this only
            // affects custom entries that forget to provide a root.)
            continue;
        };

        let data = Arc::new(widget.make_data(&root, &mut updater.params));

        let widget_m = widget.clone();
        let data_m = data.clone();
        let measure: Box<
            dyn Fn(&egui::Context, &PerfUiRowFonts, Option<f32>, &PerfUiRoot) -> f32
                + Send
                + Sync,
        > = Box::new(move |ctx, fonts, cached, root| {
            widget_m.natural_width(
                root,
                fonts,
                &data_m,
                cached,
                &mut |s: &str, fid: &egui::FontId| {
                    ctx.fonts_mut(|f| {
                        f.layout_no_wrap(s.to_owned(), fid.clone(), egui::Color32::WHITE)
                    })
                    .size()
                    .x
                },
            )
        });

        let widget_d = widget.clone();
        let draw: Box<dyn Fn(&PerfUiRoot, &mut egui::Ui, &PerfUiRowCtx<'_>) -> f32 + Send + Sync> =
            Box::new(move |root, ui, row| widget_d.render(root, ui, row, &data));

        render_data.entries.push(PreparedWidget {
            root_entity,
            sort: widget.sort_key(),
            natural_width: 0.0,
            measure,
            draw,
        });
    }
}

/// Render system: draws all Perf UI instances using egui.
///
/// Runs inside [`bevy_egui::EguiPrimaryContextPass`], after the
/// [`PerfUiSet::Update`] systems have prepared the widget data.
///
/// For each [`PerfUiRoot`], this performs a measurement pre-pass (using font
/// metrics, and a cache of previously-drawn custom widget widths) to compute
/// the natural width of every row, and the resulting panel width; then draws
/// all rows, filling them with flex space so that all rows of a panel have
/// exactly the same width (like the original, bevy_ui-based implementation).
pub(crate) fn render_perf_ui(
    mut contexts: EguiContexts,
    mut render_data: ResMut<PerfUiRenderData>,
    q_roots: Query<&PerfUiRoot>,
) {
    if render_data.entries.is_empty() {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let entries = &mut render_data.entries;

    // Order the entries by: z-index of their root, then root entity,
    // then the widget's own sort key. This groups widgets by root
    // (all widgets of a root share the same root entity id), while
    // drawing roots with a larger z-index later / on top.
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| {
        let za = z_index_of(&q_roots, entries[a].root_entity);
        let zb = z_index_of(&q_roots, entries[b].root_entity);
        za.cmp(&zb)
            .then_with(|| entries[a].root_entity.cmp(&entries[b].root_entity))
            .then_with(|| entries[a].sort.cmp(&entries[b].sort))
    });

    let mut idx = 0;
    while idx < order.len() {
        let root_entity = entries[order[idx]].root_entity;
        let start = idx;
        while idx < order.len() && entries[order[idx]].root_entity == root_entity {
            idx += 1;
        }
        // Copy the group indices, so we can access `entries` mutably below.
        let group: Vec<usize> = order[start..idx].to_vec();

        let Ok(root) = q_roots.get(root_entity) else {
            // Root component vanished between Update and rendering: skip group.
            continue;
        };
        let root = root.clone();

        // Row fonts: no custom fonts registered for now (fallback to proportional)
        let row_fonts = PerfUiRowFonts::default();

        // Pre-pass: measure the natural width of every row of this panel.
        let mut panel_width = 0.0f32;
        for &i in &group {
            let natural = (entries[i].measure)(ctx, &row_fonts, None, &root);
            entries[i].natural_width = natural;
            panel_width = panel_width.max(natural);
        }

        let margin = root.margin;
        let offset = egui::vec2(
            match root.position {
                PerfUiPosition::TopLeft | PerfUiPosition::BottomLeft => margin,
                PerfUiPosition::TopRight | PerfUiPosition::BottomRight => -margin,
            },
            match root.position {
                PerfUiPosition::TopLeft | PerfUiPosition::TopRight => margin,
                PerfUiPosition::BottomLeft | PerfUiPosition::BottomRight => -margin,
            },
        );
        let layout = if root.layout_horizontal {
            egui::Layout::left_to_right(egui::Align::Center)
        } else {
            egui::Layout::top_down(egui::Align::Min)
        };

        let mut drawn: Vec<(i32, f32)> = Vec::with_capacity(group.len());

        egui::Area::new(egui::Id::new(("perf_ui", root_entity)))
            .anchor(root.egui_anchor(), offset)
            .constrain(false)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(to_egui_color(root.background_color))
                    .inner_margin(root.padding)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing =
                            egui::vec2(root.inner_margin, root.inner_margin);
                        ui.with_layout(layout, |ui| {
                            for &i in &group {
                                let row = PerfUiRowCtx {
                                    content_width: if root.layout_horizontal {
                                        entries[i].natural_width
                                    } else {
                                        panel_width
                                    },
                                    natural: entries[i].natural_width,
                                    fonts: &row_fonts,
                                };
                                let natural = (entries[i].draw)(&root, ui, &row);
                                drawn.push((entries[i].sort, natural));
                            }
                        });
                    });
            });
        }
}

/// Helper: get the drawing z-index of the root with the given entity,
/// or 0 (fallback) if the root vanished already.
fn z_index_of(q_roots: &Query<&PerfUiRoot>, root_entity: Entity) -> i32 {
    q_roots
        .get(root_entity)
        .map(|root| root.z_index)
        .unwrap_or(0)
}
