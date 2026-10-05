use crate::api::test_support::RequestAdapter;
use crate::runtime::Execution;
use crate::storage::Config;
use crate::test_support::live::Streams;
use crate::views::app_shell::{AppShell, open};
use gpui_kit::{App, Entity, Window};
use std::sync::Arc;

pub(super) fn open_controlled(
    window: &mut Window,
    cx: &mut App,
    api: Arc<dyn RequestAdapter>,
) -> Entity<AppShell> {
    open_with_streams(window, cx, api, &Streams::default())
}

pub(super) fn open_with_streams(
    window: &mut Window,
    cx: &mut App,
    api: Arc<dyn RequestAdapter>,
    streams: &Streams,
) -> Entity<AppShell> {
    let execution = Execution::controlled(cx.background_executor().clone(), 1_800_000_000);
    open(
        window,
        cx,
        streams.transport(api),
        Config::default(),
        None,
        execution,
    )
}
