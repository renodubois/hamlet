//! GPUI list measurement for history: row-height hints, removal-safe tail padding, known
//! fitting viewports and stable-ID splices that preserve the reader's anchor.
use super::super::message_row::message_row_at;
use crate::api::Message;
use chrono::{DateTime, Local};
use gpui_kit::*;

pub(super) fn hint_history_row_heights(list: &ListState) {
    list.clone().with_uniform_item_height(px(64.));
}
// Compute presentation-only tail space before GPUI can backfill preceding rows.
// Measuring at the current width avoids stale bounds/hints after a resize. Stop
// as soon as actual rows fill the viewport; never measure the entire history.
pub(super) fn tail_anchor_padding(
    list: &ListState,
    rows: &[Message],
    now: &DateTime<Local>,
    viewport: Size<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> Pixels {
    let mut anchor = list.logical_scroll_top();
    let mut tail_height = -anchor.offset_in_item;
    for (ix, row) in rows.iter().enumerate().skip(anchor.item_ix) {
        let mut element = message_row_at(row, now, cx).into_any_element();
        let height = element
            .layout_as_root(
                size(viewport.width.into(), AvailableSpace::MinContent),
                window,
                cx,
            )
            .height;
        if ix == anchor.item_ix {
            let offset = anchor.offset_in_item.min((height - px(1.)).max(px(0.)));
            if offset != anchor.offset_in_item {
                tail_height += anchor.offset_in_item - offset;
                anchor.offset_in_item = offset;
                list.scroll_to(anchor);
            }
        }
        tail_height += height;
        if tail_height >= viewport.height {
            break;
        }
    }
    // At least one pixel of a surviving anchor remains visible. Space is bounded
    // by one viewport, not deleted row heights; appends automatically consume it.
    (viewport.height - tail_height)
        .max(px(0.))
        .min((viewport.height - px(1.)).max(px(0.)))
}

// A single presentation measurement, not a row store. Content/timestamp changes
// advance the revision; geometry and deliberate scrolling change the key.
pub(super) struct TailPadding {
    pub viewport: Size<Pixels>,
    pub revision: u64,
    pub anchor: ListOffset,
    pub padding: Pixels,
}

// None also means unknown measurements. Only a measured first-to-last viewport
// at offset zero establishes the distinct, known non-scrollable case.
pub(super) fn known_fitting_viewport(list: &ListState) -> bool {
    let anchor = list.logical_scroll_top();
    if anchor.item_ix != 0 || anchor.offset_in_item != px(0.) {
        return false;
    }
    let Some(last_ix) = list.item_count().checked_sub(1) else {
        return false;
    };
    let viewport = list.viewport_bounds();
    list.bounds_for_item(0)
        .zip(list.bounds_for_item(last_ix))
        .is_some_and(|(first, last)| {
            first.top() == viewport.top() && last.bottom() <= viewport.bottom()
        })
}

// Chronological stable-ID splices preserve variable-height viewport anchors.
pub(super) fn reconcile_history_list(list: &ListState, before: &[String], after: &[String]) {
    let positions: std::collections::HashMap<_, _> = after
        .iter()
        .enumerate()
        .map(|(ix, id)| (id.as_str(), ix))
        .collect();
    let anchor = list.logical_scroll_top();
    let surviving_anchor = before
        .iter()
        .enumerate()
        .filter_map(|(ix, id)| positions.get(id.as_str()).map(|&new_ix| (ix, new_ix)))
        // Prefer the following row on equal distance; otherwise use the nearest
        // survivor in the previously displayed chronology, not a newly inserted row.
        .min_by_key(|&(ix, _)| (ix.abs_diff(anchor.item_ix), ix < anchor.item_ix))
        .filter(|_| anchor.item_ix < before.len())
        .map(|(old_ix, new_ix)| {
            let offset_in_item = if old_ix == anchor.item_ix {
                anchor.offset_in_item
            } else {
                // A deleted row's partial offset may exceed its replacement's
                // height. Keep at least one pixel of the measured survivor visible.
                // bounds_for_item cannot measure an offscreen preceding survivor;
                // start that fallback at its top instead of guessing its height.
                list.bounds_for_item(old_ix)
                    .map(|bounds| {
                        anchor
                            .offset_in_item
                            .min((bounds.size.height - px(1.)).max(px(0.)))
                    })
                    .unwrap_or(px(0.))
            };
            ListOffset {
                item_ix: new_ix,
                offset_in_item,
            }
        });
    let mut old = 0;
    let mut new = 0;
    // Common IDs stay in chronological order. Splice only the gaps between
    // survivors, retaining their measured heights and GPUI viewport anchors.
    for (old_ix, id) in before.iter().enumerate() {
        let Some(&new_ix) = positions.get(id.as_str()) else {
            continue;
        };
        if old_ix > old || new_ix > new {
            list.splice(new..new + old_ix - old, new_ix - new);
        }
        old = old_ix + 1;
        new = new_ix + 1;
    }
    if old < before.len() || new < after.len() {
        list.splice(new..new + before.len() - old, after.len() - new);
    }
    if let Some(anchor) = surviving_anchor {
        list.scroll_to(anchor);
    }
}
