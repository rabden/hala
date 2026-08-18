//! Context-window usage ring for the conversation composer.
//!
//! Renders a circular progress indicator displaying the percentage of the
//! active context window consumed (`used / size`), with exact values in a
//! tooltip and clear color thresholds (normal neutral, warning orange >= 80%,
//! critical red >= 95%).

use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, ParentElement, PathBuilder, SharedString,
    StatefulInteractiveElement, Styled, canvas, div, hsla, point, px,
};
use zeron_proto::ContextUsage;

use crate::theme::Theme;

/// Thresholds per issue #137:
/// - Normal: < 80%
/// - Warning: 80%–94%
/// - Critical: >= 95%
pub const WARNING_THRESHOLD: f32 = 0.80;
pub const CRITICAL_THRESHOLD: f32 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextUsageSeverity {
    Normal,
    Warning,
    Critical,
}

pub fn context_usage_ratio(usage: &ContextUsage) -> f32 {
    if usage.size == 0 {
        return 0.0;
    }
    (usage.used as f32 / usage.size as f32).clamp(0.0, 1.0)
}

pub fn context_usage_percentage(usage: &ContextUsage) -> u32 {
    let ratio = context_usage_ratio(usage);
    (ratio * 100.0).round() as u32
}

pub fn context_usage_severity(usage: &ContextUsage) -> ContextUsageSeverity {
    // Thresholded on the ROUNDED percentage — the same number the tooltip
    // and label show — so the ring's color can never disagree with the
    // "80%"/"95%" the user reads at an edge.
    let pct = context_usage_percentage(usage);
    if pct as f32 / 100.0 >= CRITICAL_THRESHOLD {
        ContextUsageSeverity::Critical
    } else if pct as f32 / 100.0 >= WARNING_THRESHOLD {
        ContextUsageSeverity::Warning
    } else {
        ContextUsageSeverity::Normal
    }
}

pub fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        let val = tokens as f64 / 1_000_000.0;
        if (val * 10.0).round() % 10.0 == 0.0 {
            format!("{:.0}M", val)
        } else {
            format!("{:.1}M", val)
        }
    } else if tokens >= 1_000 {
        let val = tokens as f64 / 1_000.0;
        if (val * 10.0).round() % 10.0 == 0.0 {
            format!("{:.0}K", val)
        } else {
            format!("{:.1}K", val)
        }
    } else {
        tokens.to_string()
    }
}

pub fn context_tooltip_text(usage: &ContextUsage) -> String {
    let used = format_tokens(usage.used);
    let size = format_tokens(usage.size);
    let pct = context_usage_percentage(usage);
    // `~` marks a harness-side estimate (grok's chunk counter) so it never
    // presents as an exact measurement.
    let marker = if usage.estimated { "~" } else { "" };
    // The state word keeps warning/critical distinguishable without
    // relying on the ring's color alone.
    let state = match context_usage_severity(usage) {
        ContextUsageSeverity::Normal => "",
        ContextUsageSeverity::Warning => " — warning",
        ContextUsageSeverity::Critical => " — critical",
    };
    format!("{marker}{used} / {size} context used ({pct}%){state}")
}

pub fn context_accessible_label(usage: &ContextUsage) -> String {
    let used = format_tokens(usage.used);
    let size = format_tokens(usage.size);
    let pct = context_usage_percentage(usage);
    let marker = if usage.estimated { "~" } else { "" };
    match context_usage_severity(usage) {
        ContextUsageSeverity::Normal => format!("Context usage: {pct}%, {marker}{used} of {size}"),
        ContextUsageSeverity::Warning => {
            format!("Context usage warning: {pct}%, {marker}{used} of {size}")
        }
        ContextUsageSeverity::Critical => {
            format!("Context usage critical: {pct}%, {marker}{used} of {size}")
        }
    }
}

pub fn context_ring_color(usage: &ContextUsage, theme: &Theme) -> Hsla {
    match context_usage_severity(usage) {
        ContextUsageSeverity::Normal => theme.text_muted,
        // Amber/orange warning
        ContextUsageSeverity::Warning => hsla(38.0 / 360.0, 0.92, 0.50, 1.0),
        // Red critical
        ContextUsageSeverity::Critical => hsla(0.0 / 360.0, 0.72, 0.51, 1.0),
    }
}

/// Hover-fade key shared by the ring's `on_hover` listener and the
/// tooltip-visibility read in [`render_context_usage_ring`].
const HOVER_KEY: &str = "composer-context-usage";

/// Tooltip card pinned to the LEFT of the ring. gpui's built-in `.tooltip()`
/// tracks the cursor with no direction control, so the ring renders its own
/// hover card instead, anchored by an absolute offset inside its relative box.
fn context_tooltip_card(text: SharedString, theme: &Theme) -> gpui::Div {
    div()
        .absolute()
        // Vertically centered on the 28px ring: 24px card → top 2px.
        .top(px(2.0))
        // The card's RIGHT edge parks 6px left of the ring's left edge and
        // grows leftward — a fixed direction, never cursor-following.
        .right(px(34.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .px(px(8.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.surface_raised)
        .text_size(px(11.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.text)
        .child(text)
}

/// Render the circular gauge for context usage; lives in the composer footer
/// row beside the branch chip, with a hover tooltip pinned to its left.
pub fn render_context_usage_ring(usage: ContextUsage, theme: &Theme) -> AnyElement {
    let aria_label: SharedString = context_accessible_label(&usage).into();
    let ring_color = context_ring_color(&usage, theme);
    let track_color = crate::theme::ink(0.08);
    let ratio = context_usage_ratio(&usage);

    // Dimensions: 28px hit container, 14px outer circle with 2px stroke.
    let size_px = 28.0;
    let radius = 6.0;
    let stroke_width = 1.75;

    // The hover fade doubles as the show delay: the card mounts only once the
    // 150ms fade has settled, so a pass-through swipe never flashes it, and
    // re-renders mid-hover (streaming updates) can't kill it — the fade state
    // is keyed globally, not per element instance.
    let tooltip = (crate::motion::hover_t(HOVER_KEY) >= 0.999)
        .then(|| context_tooltip_card(context_tooltip_text(&usage).into(), theme));

    div()
        .id(HOVER_KEY)
        // Progress indicator semantics: the label carries the percentage,
        // values, and state so the gauge is usable without the ring's color.
        .role(gpui::Role::ProgressIndicator)
        .aria_label(aria_label)
        .relative()
        .size(px(size_px))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_default()
        .on_hover(crate::motion::hover_listener(HOVER_KEY))
        .children(tooltip)
        .child(
            canvas(
                move |_, _, _| {},
                move |bounds, _, window, _| {
                    let center_x = bounds.origin.x + px(size_px / 2.0);
                    let center_y = bounds.origin.y + px(size_px / 2.0);

                    // 1. Draw background track circle
                    let mut track_builder = PathBuilder::stroke(px(stroke_width));
                    track_builder.move_to(point(center_x + px(radius), center_y));
                    track_builder.arc_to(
                        point(px(radius), px(radius)),
                        px(0.),
                        false,
                        true,
                        point(center_x - px(radius), center_y),
                    );
                    track_builder.arc_to(
                        point(px(radius), px(radius)),
                        px(0.),
                        false,
                        true,
                        point(center_x + px(radius), center_y),
                    );
                    if let Ok(path) = track_builder.build() {
                        window.paint_path(path, track_color);
                    }

                    // 2. Draw progress arc starting from top (-90 degrees / 12 o'clock)
                    if ratio > 0.001 {
                        let mut arc_builder = PathBuilder::stroke(px(stroke_width));
                        let start_angle = -std::f32::consts::FRAC_PI_2;
                        let sweep_angle = ratio * std::f32::consts::TAU;

                        let start_x = center_x + px(radius * start_angle.cos());
                        let start_y = center_y + px(radius * start_angle.sin());
                        arc_builder.move_to(point(start_x, start_y));

                        if ratio >= 0.999 {
                            // Full circle: two semicircles
                            arc_builder.arc_to(
                                point(px(radius), px(radius)),
                                px(0.),
                                false,
                                true,
                                point(center_x, center_y + px(radius)),
                            );
                            arc_builder.arc_to(
                                point(px(radius), px(radius)),
                                px(0.),
                                false,
                                true,
                                point(start_x, start_y),
                            );
                        } else {
                            let end_angle = start_angle + sweep_angle;
                            let end_x = center_x + px(radius * end_angle.cos());
                            let end_y = center_y + px(radius * end_angle.sin());
                            let large_arc = sweep_angle > std::f32::consts::PI;

                            arc_builder.arc_to(
                                point(px(radius), px(radius)),
                                px(0.),
                                large_arc,
                                true,
                                point(end_x, end_y),
                            );
                        }

                        if let Ok(path) = arc_builder.build() {
                            window.paint_path(path, ring_color);
                        }
                    }
                },
            )
            .size(px(size_px)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_usage_calculations_and_thresholds() {
        let normal = ContextUsage {
            used: 42_000,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&normal), 21);
        assert_eq!(
            context_usage_severity(&normal),
            ContextUsageSeverity::Normal
        );
        assert_eq!(
            context_tooltip_text(&normal),
            "42K / 200K context used (21%)"
        );
        assert_eq!(
            context_accessible_label(&normal),
            "Context usage: 21%, 42K of 200K"
        );

        let warning = ContextUsage {
            used: 168_000,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&warning), 84);
        assert_eq!(
            context_usage_severity(&warning),
            ContextUsageSeverity::Warning
        );
        assert_eq!(
            context_tooltip_text(&warning),
            "168K / 200K context used (84%) — warning"
        );
        assert_eq!(
            context_accessible_label(&warning),
            "Context usage warning: 84%, 168K of 200K"
        );

        let critical = ContextUsage {
            used: 194_000,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&critical), 97);
        assert_eq!(
            context_usage_severity(&critical),
            ContextUsageSeverity::Critical
        );
        assert_eq!(
            context_tooltip_text(&critical),
            "194K / 200K context used (97%) — critical"
        );
        assert_eq!(
            context_accessible_label(&critical),
            "Context usage critical: 97%, 194K of 200K"
        );

        // Estimates carry the `~` marker in both surfaces.
        let estimated = ContextUsage {
            used: 13_667,
            size: 200_000,
            estimated: true,
        };
        assert_eq!(
            context_tooltip_text(&estimated),
            "~13.7K / 200K context used (7%)"
        );
        assert_eq!(
            context_accessible_label(&estimated),
            "Context usage: 7%, ~13.7K of 200K"
        );
    }

    #[test]
    fn severity_thresholds_follow_the_rounded_percentage() {
        // Exactly the issue's suggested edges: 80% warns, 94% still warns,
        // 95% goes critical.
        let at_80 = ContextUsage {
            used: 160_000,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&at_80), 80);
        assert_eq!(
            context_usage_severity(&at_80),
            ContextUsageSeverity::Warning
        );

        let at_94 = ContextUsage {
            used: 188_000,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&at_94), 94);
        assert_eq!(
            context_usage_severity(&at_94),
            ContextUsageSeverity::Warning
        );

        let at_95 = ContextUsage {
            used: 190_000,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&at_95), 95);
        assert_eq!(
            context_usage_severity(&at_95),
            ContextUsageSeverity::Critical
        );

        // A ratio that rounds UP to the threshold takes the threshold's
        // state — the color can never disagree with the displayed "80%".
        let rounds_to_80 = ContextUsage {
            used: 159_999,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&rounds_to_80), 80);
        assert_eq!(
            context_usage_severity(&rounds_to_80),
            ContextUsageSeverity::Warning
        );

        let rounds_to_95 = ContextUsage {
            used: 189_999,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&rounds_to_95), 95);
        assert_eq!(
            context_usage_severity(&rounds_to_95),
            ContextUsageSeverity::Critical
        );

        // Just under the threshold (displays 79%) stays normal.
        let at_79 = ContextUsage {
            used: 158_600,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_percentage(&at_79), 79);
        assert_eq!(context_usage_severity(&at_79), ContextUsageSeverity::Normal);
    }

    #[test]
    fn ratio_edges_zero_window_and_overfull_gauge() {
        // Zero window is "unavailable", never a divide-by-zero.
        let zero = ContextUsage {
            used: 0,
            size: 0,
            estimated: false,
        };
        assert_eq!(context_usage_ratio(&zero), 0.0);
        assert_eq!(context_usage_percentage(&zero), 0);
        assert_eq!(context_usage_severity(&zero), ContextUsageSeverity::Normal);

        // used > size (a mis-report that slipped past the harness cap)
        // clamps to a full ring, never >100%.
        let overfull = ContextUsage {
            used: 318_000,
            size: 200_000,
            estimated: false,
        };
        assert_eq!(context_usage_ratio(&overfull), 1.0);
        assert_eq!(context_usage_percentage(&overfull), 100);
        assert_eq!(
            context_usage_severity(&overfull),
            ContextUsageSeverity::Critical
        );
    }

    #[test]
    fn format_tokens_scale() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1_000), "1K");
        assert_eq!(format_tokens(42_000), "42K");
        assert_eq!(format_tokens(200_000), "200K");
        assert_eq!(format_tokens(1_000_000), "1M");
        assert_eq!(format_tokens(1_200_000), "1.2M");
        assert_eq!(format_tokens(2_000_000), "2M");
    }
}
