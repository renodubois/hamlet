use gpui_kit::base::input::{InputBaseState, InputMode, InputState};
use gpui_kit::component::Root;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::*;

struct Hamlet {
    text: SharedString,
    input_val: Entity<InputBaseState<InputMode>>,
}

impl Render for Hamlet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = self.input_val.clone();
        let view = cx.entity().downgrade();
        let greeting = format!("Hello, {}!", &self.text);
        div()
            .size_full()
            .bg(rgb(0xffffff))
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .text_3xl()
            .text_color(rgb(0x0))
            .child(
                div()
                    .id("greeting")
                    .aria_label(greeting.clone())
                    .test_support()
                    .child(greeting),
            )
            .child(
                div()
                    .flex()
                    .size_full()
                    .border_2()
                    .border_color(rgb(0x0))
                    .child(Input::new(&self.input_val).id("name-input"))
                    .child(
                        Button::new("submit-name")
                            .label("Submit")
                            .on_click(move |_, _, cx| {
                                let text = input.read(cx).text().to_string();
                                let _ = view.update(cx, |view, cx| {
                                    view.text = text.into();
                                    cx.notify();
                                });
                            }),
                    ),
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

#[cfg(test)]
mod tests {
    use super::Hamlet;
    use gpui_kit::TestSupportExt as _;
    use gpui_kit::base::SelectableText;
    use gpui_kit::base::input::InputState;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AppContext as _, Context, FocusHandle, InteractiveElement as _, IntoElement, ListAlignment,
        ListState, MouseButton, ParentElement as _, Render, SharedString, Styled as _,
        TestAppContext, Window, div, list, px,
    };

    // This is a real Kit/GPUI history surface, not a simulated scrolling model.
    struct HistoryProbe {
        list: ListState,
        messages: Vec<(String, SharedString)>,
        focus: FocusHandle,
    }

    impl HistoryProbe {
        fn new(cx: &mut Context<Self>) -> Self {
            let messages =
                (0..40)
                    .map(|ix| {
                        (format!("original-{ix}"), if ix % 2 == 0 {
                    "first line\nsecond line with enough words to wrap at a narrow width"
                } else {
                    "short line"
                }.into())
                    })
                    .collect();
            Self {
                list: ListState::new(40, ListAlignment::Top, px(0.)).measure_all(),
                messages,
                focus: cx.focus_handle(),
            }
        }

        fn prepend(&mut self, cx: &mut Context<Self>) {
            self.messages.splice(
                0..0,
                [
                    ("older-0".into(), "older\nfirst line".into()),
                    ("older-1".into(), "older short".into()),
                ],
            );
            self.list.splice(0..0, 2);
            cx.notify();
        }
    }

    impl Render for HistoryProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let messages = self.messages.clone();
            let focus = self.focus.clone();
            div().w(px(260.)).h(px(180.)).child(
                div()
                    .id("history")
                    .test_support()
                    .track_focus(&self.focus)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        window.focus(&focus, cx)
                    })
                    .size_full()
                    .child(
                        list(self.list.clone(), move |ix, _, _| {
                            let (id, text) = &messages[ix];
                            div()
                                .id(format!("message-{id}"))
                                .test_support()
                                .w_full()
                                .p_2()
                                .child(SelectableText::new(
                                    format!("selectable-{id}"),
                                    text.clone(),
                                ))
                                .into_any_element()
                        })
                        .size_full(),
                    ),
            )
        }
    }

    #[gpui_kit::test]
    fn varied_height_history_preserves_reader_on_prepend(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let probe = std::rc::Rc::new(std::cell::RefCell::new(None));
        let stored = probe.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(HistoryProbe::new);
            *stored.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view: gpui_kit::Entity<HistoryProbe> = probe.borrow().as_ref().unwrap().clone();
        cx.update(|window, cx| {
            window.render_frame(cx);
            for _ in 0..8 {
                window.scroll(
                    "history",
                    gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                        gpui_kit::px(0.),
                        gpui_kit::px(-90.),
                    )),
                    cx,
                );
            }
            let before = view.read(cx).list.logical_scroll_top();
            assert!(
                before.item_ix > 0,
                "the actual wheel must scroll variable-height rows"
            );
            let id = format!("message-original-{}", before.item_ix);
            let y = window.find(id.clone()).bounds().origin.y;
            view.update(cx, |view, cx| view.prepend(cx));
            window.render_frame(cx);
            let after = view.read(cx).list.logical_scroll_top();
            assert_eq!(after.item_ix, before.item_ix + 2);
            assert_eq!(after.offset_in_item, before.offset_in_item);
            assert_eq!(window.find(id).bounds().origin.y, y);
        });
    }

    #[gpui_kit::test]
    fn selectable_history_copies_line_breaks(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(HistoryProbe::new);
            Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            let bounds = window.find("message-original-0").bounds();
            window.drag(
                bounds.origin + gpui_kit::point(gpui_kit::px(2.), gpui_kit::px(12.)),
                bounds.origin
                    + gpui_kit::point(
                        bounds.size.width - gpui_kit::px(2.),
                        bounds.size.height - gpui_kit::px(2.),
                    ),
                cx,
            );
            assert!(gpui_kit::base::TextSelection::selected_text(window, cx).contains('\n'));
            window.press("ctrl-c", cx);
        });
        assert!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap()
                .contains('\n')
        );
    }

    #[gpui_kit::test]
    fn submitting_the_real_input_updates_the_greeting(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|window, cx| Hamlet {
            text: "World".into(),
            input_val: cx.new(|cx| InputState::new(window, cx)),
        });
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.click("name-input", cx);
            window.input("Ada", cx);
            assert_eq!(window.find("name-input").value(), Some("Ada"));
            window.click("submit-name", cx);
            window.render_frame(cx);
            assert_eq!(window.find("greeting").label(), Some("Hello, Ada!"));
        });
    }
}
