//! Actions gitr's macOS menu bar dispatches.
//!
//! Every action here is registered globally, on the `App`, from `crates/gitr/src/main.rs`:
//! [`Quit`] directly, since closing the one window already calls `cx.quit()`, and the other
//! ten through `Workspace::register_menu_actions`.
//!
//! Global is the only scope a menu item can be driven from. macOS decides whether to draw
//! an item enabled by asking `App::is_action_available`, which a window answers by walking
//! its dispatch path from the focused element to the window root — a path that, with
//! nothing focused, starts *at* the root and so holds only the root. Registering these ten
//! on `Workspace`'s render root, a descendant of that root, left every one of them greyed
//! out and unclickable while `Quit` alone stayed live. See
//! `Workspace::register_menu_actions`.
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
        CloseWindow,
    ]
);
