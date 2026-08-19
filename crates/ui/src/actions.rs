//! Actions gitr's macOS menu bar dispatches.
//!
//! [`Quit`] is registered once, globally, in `crates/gitr/src/main.rs`: closing the one
//! window already calls `cx.quit()`, and the application menu's Quit item must reach the
//! same call regardless of which window, if any, currently has focus. Every other action
//! here is registered window-scoped, on `Workspace`'s render root in
//! `crates/ui/src/workspace.rs`, because each one acts on the single `Workspace` that
//! window holds.
//!
//! `Cut`, `Copy`, `Paste` and `Select All` are not defined here — the menu references
//! `gpui_component::input`'s own actions directly, since every editable region in the
//! window already registers a handler for those each time it paints.

use gpui::actions;

actions!(
    gitr,
    [
        About,
        Quit,
        ToggleSidebar,
        ToggleDetailPanel,
        OpenFromDisk,
        SynchroniseActiveProject,
        UseLightTheme,
        UseDarkTheme,
        UseSystemTheme,
        MinimizeWindow,
        ZoomWindow,
    ]
);
