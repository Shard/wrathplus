//! The `ScrollFrame` kind's state.

use super::FrameHandle;

/// A `CSimpleScrollFrame`'s members (`+0x318`/`+0x324`/`+0x328`); the ranges are measured live.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScrollFrameState {
    /// `SetScrollChild`'s frame, which pans within this frame's rect.
    pub child: Option<FrameHandle>,
    /// `SetHorizontalScroll`'s offset in px, unclamped; `0x787100` anchors the child at
    /// `(+horizontal, +vertical)`, so scrolling right takes a negative offset, down a positive.
    pub horizontal: f32,
    /// `SetVerticalScroll`'s offset in px (`0x786db0`), unclamped: a positive one lifts the child
    /// to show what is below; the FrameXML scroll bar's range keeps it in bounds.
    pub vertical: f32,
    /// The `(horizontal, vertical)` ranges `OnScrollRangeChanged` last carried, `(0, 0)` at
    /// creation; the layout pump fires again only on a change.
    pub notified: (f32, f32),
}
