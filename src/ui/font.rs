//! Registration of the custom fonts of a [`PerfUiRoot`] into egui's font
//! definitions, and resolution of the corresponding [`PerfUiRowFonts`].
//!
//! egui's [`Context::set_fonts`](egui::Context::set_fonts) is *deferred*: the
//! new definitions only become active at the start of the next egui pass (they
//! are queued and consumed by `Context::begin_pass`, which bevy_egui runs in
//! `PreUpdate` — before the systems of [`EguiPrimaryContextPass`](bevy_egui::EguiPrimaryContextPass)).
//! Laying out text with a font family that was registered in the *current*
//! pass therefore panics ("FontFamily::Name(...) is not bound to any fonts"),
//! because the family lookup runs against the still-active definitions.
//!
//! To stay on the safe side of that boundary, this module maintains two
//! states:
//!
//! - **pending**: fonts that were passed to `set_fonts` during a previous
//!   frame (their definitions are queued/just-activated, but must not be
//!   referenced from `FontId`s yet), and
//! - **active**: fonts whose definitions have been activated at a
//!   `begin_pass` boundary — only these are safe to use in `FontId`s.
//!
//! At the start of every rendered frame (the first thing [`update_perf_ui_fonts`]
//! does), everything pending is promoted to active: at that point, a
//! `begin_pass` has already run since the `set_fonts` call, so the families
//! exist in the active definitions. Rows keep using egui's default
//! proportional font until their custom fonts become active (typically the
//! second frame after the font asset finished loading).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;

use crate::ui::root::PerfUiRoot;
use crate::ui::widget::PerfUiRowFonts;

/// Resource tracking the custom fonts of all [`PerfUiRoot`]s and their
/// registration state inside egui.
///
/// See the [module documentation](self) for why registration is split into
/// two states.
#[derive(Resource, Default)]
pub(crate) struct PerfUiFontState {
    /// Fonts passed to `Context::set_fonts` during the current or a previous
    /// frame, whose definitions are not yet confirmed to be active.
    pending: HashMap<AssetId<Font>, String>,
    /// Registered egui font names, keyed by the id of the bevy font asset
    /// they were created from. Only these are safe to use in `FontId`s.
    active: HashMap<AssetId<Font>, String>,
    /// Monotonic counter used to generate unique egui font names.
    name_counter: u64,
}

impl PerfUiFontState {
    /// Move everything registered during previous frames to the active set.
    ///
    /// Must be called at the start of a frame's egui rendering (before any
    /// text is measured/drawn), i.e. after this frame's
    /// `Context::begin_pass` had the chance to activate the queued
    /// definitions.
    fn promote_pending(&mut self) {
        self.active.extend(self.pending.drain());
    }

    /// The active egui font name registered for the given font handle, if any.
    fn active_name(&self, handle: &Handle<Font>) -> Option<&String> {
        self.active.get(&handle.id())
    }
}

/// Register the fonts configured in every [`PerfUiRoot`] into egui's font
/// definitions, and promote previously-registered fonts once they are safe
/// to use.
///
/// Call this at the start of the frame's egui rendering (inside
/// [`EguiPrimaryContextPass`](bevy_egui::EguiPrimaryContextPass), before any
/// text measurement or drawing):
///
/// 1. Pending registrations from previous frames become active (their
///    definitions were applied at the `Context::begin_pass` that bevy_egui
///    runs in `PreUpdate`).
/// 2. Every root's font handles are examined; loaded, not-yet-registered
///    fonts are added to a clone of egui's *current* font definitions and
///    queued via `Context::set_fonts` (which is deferred to the next pass —
///    hence the pending/active split).
///
/// Font assets that are not loaded yet are skipped until they are (`Assets::get`
/// only returns `Some` once the asset has actually been loaded). The default
/// font handles of a [`PerfUiRoot`] are skipped explicitly: `Handle::default()`
/// resolves to Bevy's built-in default font asset, and roots without custom
/// fonts should keep using egui's proportional fallback instead of registering
/// it.
pub(crate) fn update_perf_ui_fonts(
    ctx: &egui::Context,
    state: &mut PerfUiFontState,
    font_assets: &Assets<Font>,
    q_roots: &Query<&PerfUiRoot>,
) {
    // Promote first: since the previous registration, a full egui pass
    // boundary has been crossed (begin_pass in PreUpdate of this frame).
    state.promote_pending();

    // Collect fonts that are loaded and not registered in any state yet.
    let mut to_register: Vec<(AssetId<Font>, Vec<u8>)> = Vec::new();
    let mut seen: HashSet<AssetId<Font>> = HashSet::new();
    for root in q_roots.iter() {
        for handle in [&root.font_label, &root.font_value, &root.font_highlight] {
            let id = handle.id();
            // The default handle (`Handle::default()`, id = `AssetId::default()`)
            // resolves to Bevy's built-in default font asset. Roots that do not
            // configure custom fonts must not register it as a custom font;
            // they should keep using egui's proportional fallback.
            if id == AssetId::default() {
                continue;
            }
            if state.active.contains_key(&id) || state.pending.contains_key(&id) {
                continue;
            }
            if !seen.insert(id) {
                continue;
            }
            // Only register fonts whose asset is actually loaded; unloaded
            // handles resolve to `None` here and are skipped.
            if let Some(font) = font_assets.get(handle) {
                to_register.push((id, font.data.as_ref().to_vec()));
            }
        }
    }
    if to_register.is_empty() {
        return;
    }

    // Clone egui's CURRENT definitions and extend them. The definitions must
    // never be rebuilt from `FontDefinitions::default()` once egui is running,
    // and `set_fonts` replaces the whole set — so we always start from what
    // is currently active.
    let mut defs = ctx.fonts(|f| f.definitions().clone());
    for (id, bytes) in to_register {
        // Names must be unique across the egui context; a simple counter
        // keeps them short and stable (the bevy asset id is kept in the maps
        // as the source of truth).
        let name = format!("perf_ui_font_{}", state.name_counter);
        state.name_counter += 1;

        defs.font_data
            .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
        defs.families
            .entry(egui::FontFamily::Name(name.clone().into()))
            .or_default()
            .push(name.clone());

        state.pending.insert(id, name);
    }

    // Deferred: these definitions become active at the start of the next egui
    // pass. The names stay in `pending` (never used in `FontId`s) until the
    // promotion above confirms activation.
    ctx.set_fonts(defs);
}

/// Build the [`PerfUiRowFonts`] for a [`PerfUiRoot`] from the (active) font
/// registrations.
///
/// Fonts that are not active yet (not loaded, or still pending) stay `None`,
/// which makes the row fall back to egui's proportional font.
pub(crate) fn row_fonts_for(state: &PerfUiFontState, root: &PerfUiRoot) -> PerfUiRowFonts {
    PerfUiRowFonts {
        label: state.active_name(&root.font_label).cloned(),
        value: state.active_name(&root.font_value).cloned(),
        highlight: state.active_name(&root.font_highlight).cloned(),
    }
}
