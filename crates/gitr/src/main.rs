use gpui::{App, AppContext};
use gpui_component::{Root, TitleBar};
use ui::Workspace;

fn main() {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            ui::init(cx);

            cx.spawn(async move |cx| {
                cx.open_window(TitleBar::window_options(), |window, cx| {
                    let workspace = cx.new(|cx| Workspace::new(window, cx));
                    cx.new(|cx| Root::new(workspace, window, cx))
                })
                .expect("gitr cannot run without a window");
            })
            .detach();
        });
}
