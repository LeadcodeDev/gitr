use gpui::{App, AppContext, WindowOptions};
use gpui_component::Root;
use ui::Workspace;

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let workspace = cx.new(|_| Workspace::new());
                cx.new(|cx| Root::new(workspace, window, cx))
            })
            .expect("gitr cannot run without a window");
        })
        .detach();
    });
}
