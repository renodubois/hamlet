use gpui_kit::base::Input;
use gpui_kit::base::input::{InputBaseState, InputMode, InputState};
use gpui_kit::component::Root;
use gpui_kit::component::button::Button;
use gpui_kit::*;

struct Hamlet {
    text: SharedString,
    input_val: Entity<InputBaseState<InputMode>>,
}

impl Render for Hamlet {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.input_val.read(_cx).text();
        div()
            .size_full()
            .bg(rgb(0xffffff))
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .text_3xl()
            .text_color(rgb(0x0))
            .child(format!("Hello, {}!", &self.text))
            .child(
                div()
                    .flex()
                    .size_full()
                    .border_2()
                    .border_color(rgb(0x0))
                    .child(Input::new(&self.input_val))
                    .child(Button::new("primary-button").child("Submit")),
            )
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);

            let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    let input_state = cx.new(|cx| InputState::new(window, cx));

                    let view = cx.new(|_| Hamlet {
                        text: "World".into(),
                        input_val: input_state,
                    });

                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
