//! The density scale applied on top of gpui-component's defaults.
//!
//! gpui-component ships sized for a web body (`font_size: 16px`, `radius: 6px`, 32px
//! table rows). Every one of those numbers reads oversized in a desktop utility window,
//! so [`apply`] overrides them once, at startup, rather than letting call sites reach for
//! `px(12.)` individually. [`crate::init`] is what calls [`apply`], and it must run after
//! `gpui_component::init(cx)` — that call installs the `Theme` global this overwrites.

use gpui::{App, Pixels, px};
use gpui_component::Theme;

/// Base text size. gpui-component's `text_sm`/`text_xs`/... are `rem`-relative, and
/// `Root` sets the window's rem size from this field, so lowering it cascades through
/// every component that asks for a relative size rather than a literal pixel count.
pub const FONT_SIZE: Pixels = px(12.);

/// Size for `Menlo`/`Consolas`/`DejaVu Sans Mono` text: commit SHAs, diff bodies.
pub const MONO_FONT_SIZE: Pixels = px(11.);

/// Corner radius for buttons, inputs, badges and panels. At the control sizes this pass
/// produces, gpui-component's default 6px reads as a pill rather than a squared-off
/// corner; 3px keeps a corner visible as a corner.
pub const RADIUS: Pixels = px(3.);

/// History table row height, via `DataTable::with_size`. Below `Size::XSmall`'s 26px
/// preset, which is why it is reached through `Size::Size(TABLE_ROW_HEIGHT)` rather than
/// a named preset.
///
/// Not 22px: gpui-component fixes a `Size::Size` cell's padding at 4px top and bottom
/// regardless of the row height requested, and a reference badge (`Tag::xsmall`) resolves
/// to a 15px glyph — border, padding and a `relative(1.)` line height included. 22px
/// leaves that badge 1px taller than its content box and lets the table's own
/// `overflow_hidden` clip it. 24px is the smallest 4px-grid value with headroom.
pub const TABLE_ROW_HEIGHT: Pixels = px(24.);

/// Row height for the sidebar's reference tree. `gpui_component::sidebar::SidebarMenuItem`
/// hard-codes `h_7()` (28px), so this crate's own row (`sidebar::tree`) reads this
/// constant directly instead of following the theme.
pub const SIDEBAR_ROW_HEIGHT: Pixels = px(20.);

/// Horizontal indent added per nesting depth in the sidebar's reference tree.
pub const SIDEBAR_INDENT: Pixels = px(10.);

/// Height the commit detail panel's description region caps out at before it grows its
/// own scrollbar, so an unusually long commit body cannot squeeze the diff editor beneath
/// it out of the panel. About 15 lines at [`FONT_SIZE`] with default line height — enough
/// for a typical multi-paragraph message without one dominating the fixed-height dock.
pub const DETAIL_DESCRIPTION_MAX_HEIGHT: Pixels = px(220.);

/// Applies the density scale to the global [`Theme`].
///
/// Must run after `gpui_component::init(cx)` installs the `Theme` global. Safe to call
/// again after a mode switch: none of gpui-component's bundled theme JSON sets
/// `font.size`, `mono_font.size` or `radius`, so `Theme::apply_config` — which runs on
/// every `Theme::change`, including the light/dark toggle and
/// `Theme::sync_system_appearance` — never overwrites these fields back to its own
/// defaults. Reapplying here is therefore redundant given today's bundled themes, not
/// load-bearing; it stays cheap insurance against a future theme file that does set them.
pub fn apply(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    theme.font_size = FONT_SIZE;
    theme.mono_font_size = MONO_FONT_SIZE;
    theme.radius = RADIUS;
}
