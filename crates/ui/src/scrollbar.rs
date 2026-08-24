//! Geometry and pointer math for the floating scrollbar rails — the model
//! picker's list rail and the transcript's thought-box rails are the same
//! widget: an inset track, a 3px thumb that widens to 5px while hovered or
//! dragged, a 24px minimum thumb, inside a fixed-width hit strip. Listeners
//! stay at each call site (they hang off different scroll machinery); the
//! numbers, their inverses, and the thumb element live here.

use crate::theme::Theme;
use gpui::{div, px, Styled};

/// Track inset from both ends of the rail.
pub const TRACK_INSET: f32 = 4.0;
/// Full-width hover/click strip containing the visible thumb.
pub const HIT_WIDTH: f32 = 10.0;
/// Thumb width at rest.
pub const THUMB_WIDTH: f32 = 3.0;
/// Thumb width while hovered or dragged.
pub const HOVER_THUMB_WIDTH: f32 = 5.0;
/// Lower bound on thumb length so a huge document keeps a grabbable handle.
pub const MIN_THUMB: f32 = 24.0;

/// Thumb placement for one rail in one frame. `top`/`height`/`travel` are
/// track-local pixels; `max_scroll` is the matching content distance so the
/// inverse mapping ([`scroll_for_pointer`]) needs nothing else.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThumbMetrics {
    pub track_height: f32,
    pub thumb_top: f32,
    pub thumb_height: f32,
    pub travel: f32,
    pub max_scroll: f32,
}

/// Proportional thumb math for a viewport of `viewport_h` px onto `content_h`
/// px at `scroll_top`. `None` when there is no overflow — short content gets
/// no rail at all.
pub fn metrics_for(scroll_top: f32, content_h: f32, viewport_h: f32) -> Option<ThumbMetrics> {
    let max_scroll = (content_h - viewport_h).max(0.0);
    if viewport_h <= 0.0 || content_h <= 0.0 || max_scroll <= 0.0 {
        return None;
    }
    let track_height = (viewport_h - TRACK_INSET * 2.0).max(0.0);
    if track_height <= 0.0 {
        return None;
    }
    let thumb_height = (track_height * viewport_h / content_h)
        .max(MIN_THUMB)
        .min(track_height);
    let travel = (track_height - thumb_height).max(0.0);
    let thumb_top = travel * scroll_top.clamp(0.0, max_scroll) / max_scroll;
    Some(ThumbMetrics {
        track_height,
        thumb_top,
        thumb_height,
        travel,
        max_scroll,
    })
}

/// Where a pointer press sits relative to the thumb top: inside the thumb it
/// preserves the offset within it; on the bare track it centers the thumb
/// under the pointer first (so a click both jumps and starts a drag).
pub fn grab_offset_for(pointer_in_track: f32, m: &ThumbMetrics) -> f32 {
    if (m.thumb_top..=m.thumb_top + m.thumb_height).contains(&pointer_in_track) {
        pointer_in_track - m.thumb_top
    } else {
        m.thumb_height / 2.0
    }
}

/// Inverse of [`metrics_for`] for pointer-driven moves: a track-local pointer
/// position minus its grab offset is the thumb top, and thumb position maps
/// linearly back to a scroll offset.
pub fn scroll_for_pointer(pointer_in_track: f32, grab_offset: f32, m: ThumbMetrics) -> f32 {
    let thumb_top = (pointer_in_track - grab_offset).clamp(0.0, m.travel);
    if m.travel <= 0.0 {
        0.0
    } else {
        thumb_top / m.travel * m.max_scroll
    }
}

/// The visible thumb: an absolute child inside the fixed-width hit rail, so
/// hover expansion never reflows rows.
pub fn thumb(m: &ThumbMetrics, active: bool, theme: &Theme) -> gpui::Div {
    let width = if active { HOVER_THUMB_WIDTH } else { THUMB_WIDTH };
    div()
        .absolute()
        .top(px(TRACK_INSET + m.thumb_top))
        .right(px(2.0))
        .w(px(width))
        .h(px(m.thumb_height))
        .rounded(px(width / 2.0))
        .bg(theme.text_faint.opacity(if active { 0.68 } else { 0.5 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_scales_the_thumb_like_the_picker() {
        // A short (non-overflowing) viewport gets no rail at all.
        assert_eq!(metrics_for(0.0, 390.0, 400.0), None);

        let m = metrics_for(0.0, 2000.0, 400.0).expect("overflow gets a rail");
        assert_eq!(m.max_scroll, 1600.0);
        let track = 400.0 - TRACK_INSET * 2.0;
        assert!((m.track_height - track).abs() < 1e-3);
        assert!((m.thumb_height - track * 400.0 / 2000.0).abs() < 1e-3);
        assert!((m.travel - (track - m.thumb_height)).abs() < 1e-3);
        assert_eq!(m.thumb_top, 0.0);

        // A huge document clamps the thumb to the shared minimum.
        let tiny = metrics_for(0.0, 100_000.0, 400.0).expect("huge doc gets a rail");
        assert!((tiny.thumb_height - MIN_THUMB).abs() < 1e-3);

        // Pinned to the tail, the thumb rides to the end of its travel.
        let bottom = metrics_for(1600.0, 2000.0, 400.0).unwrap();
        assert!((bottom.thumb_top - bottom.travel).abs() < 1e-3);
    }

    #[test]
    fn grab_offset_preserves_inside_the_thumb_and_centers_on_track() {
        let m = metrics_for(0.0, 2000.0, 400.0).unwrap();
        // Inside the thumb: keep the pointer's position relative to its top.
        let inside = m.thumb_top + m.thumb_height * 0.25;
        assert!((grab_offset_for(inside, &m) - m.thumb_height * 0.25).abs() < 1e-3);
        // On the bare track below: center the thumb first.
        let below = m.thumb_top + m.thumb_height + 40.0;
        assert!((grab_offset_for(below, &m) - m.thumb_height / 2.0).abs() < 1e-3);
    }

    #[test]
    fn scroll_for_pointer_centers_a_track_click() {
        let m = metrics_for(0.0, 2000.0, 400.0).unwrap();
        // A click below the thumb centers it first: half travel → half range.
        let pointer = m.travel * 0.5 + m.thumb_height / 2.0;
        let next = scroll_for_pointer(pointer, m.thumb_height / 2.0, m);
        assert!((next - m.max_scroll * 0.5).abs() < 1e-3);
        // Grabbed-drag keeps the offset; ends clamp.
        assert_eq!(scroll_for_pointer(-100.0, 0.0, m), 0.0);
        assert_eq!(scroll_for_pointer(m.travel + 100.0, 0.0, m), m.max_scroll);
    }
}
