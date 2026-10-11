//! Observe the real text leaf's layout input and paint, not an ARIA label or
//! parent style. The ordinary GPUI string still performs all layout/painting.
use gpui_kit::*;

pub(crate) struct PaintedMenuText {
    pub(crate) text: String,
    pub(crate) color: Hsla,
}
impl Global for PaintedMenuText {}

pub(crate) struct MenuText(pub(crate) &'static str);
impl IntoElement for MenuText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for MenuText {
    type RequestLayoutState = (TextLayout, Hsla);
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let color = window.text_style().color;
        let (layout, state) = self.0.request_layout(id, inspector, window, cx);
        (layout, (state, color))
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0
            .prepaint(id, inspector, bounds, &mut state.0, window, cx);
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0
            .paint(id, inspector, bounds, &mut state.0, prepaint, window, cx);
        cx.set_global(PaintedMenuText {
            text: state.0.text(),
            color: state.1,
        });
    }
}
