//! Slider control over Rune primitives and kit tokens.
//!
//! Owns the track/thumb geometry, pointer press-and-drag value picking, and
//! keyboard stepping. Value state and the `on_change` callback are owned by the
//! caller, matching React's controlled `Slider`.
//!
//! ## Keys
//!
//! | part  | key           |
//! |-------|---------------|
//! | track | `{key}`       |
//! | thumb | `{key}-thumb` |

#![allow(dead_code)]

use std::rc::Rc;

use rune::{el, row, Drag, El, Key, KeyEvent, Phase, Tx};

use crate::ui::kit::kit;

fn snap(min: f64, max: f64, step: f64, ratio: f64) -> f64 {
    let range = max - min;
    if range <= 0.0 || step <= 0.0 {
        return min.clamp(min.min(max), max.max(min));
    }
    let ratio = ratio.clamp(0.0, 1.0);
    let stepped = min + ((ratio * range) / step).round() * step;
    stepped.clamp(min.min(max), max.max(min))
}

fn ratio_for(value: f64, min: f64, max: f64) -> f64 {
    let range = max - min;
    if range <= 0.0 {
        return 0.0;
    }
    ((value - min) / range).clamp(0.0, 1.0)
}

type OnChange<S> = Rc<dyn Fn(&mut S, f64)>;

pub(crate) struct Slider<S: 'static> {
    key: String,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    disabled: bool,
    label: Option<String>,
    width: Option<f32>,
    on_change: Option<OnChange<S>>,
}

impl<S: 'static> Slider<S> {
    pub(crate) fn new(key: impl Into<String>, value: f64) -> Self {
        Self {
            key: key.into(),
            value,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            disabled: false,
            label: None,
            width: None,
            on_change: None,
        }
    }

    /// Create a slider with value 0.0.
    pub(crate) fn from_key(key: impl Into<String>) -> Self {
        Self::new(key, 0.0)
    }

    pub(crate) fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    pub(crate) fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    pub(crate) fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    pub(crate) fn step(mut self, step: f64) -> Self {
        self.step = if step <= 0.0 { 1.0 } else { step };
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Fixed track width in points. When set, thumb/fill are positioned so the
    /// thumb center sits at `ratio * width` from the track's left edge.
    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub(crate) fn on_change(mut self, f: impl Fn(&mut S, f64) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let c = &k.c;

        let min = self.min;
        let max = self.max;
        let step = self.step;
        let value = self.value.clamp(min.min(max), max.max(min));
        let ratio = ratio_for(value, min, max);

        let track_h = 4.0_f32;
        let thumb_size = 16.0_f32;
        let hit_h = 36.0_f32;
        let track_radius = track_h / 2.0;
        let track_y = (hit_h - track_h) / 2.0;
        let thumb_y = (hit_h - thumb_size) / 2.0;

        let track_bg = el()
            .key(format!("{}-track-bg", self.key))
            .absolute(0.0, track_y)
            .w_full()
            .h(track_h)
            .radius(track_radius)
            .fill(c.surface_raised);

        let (fill_el, thumb_el) = if let Some(w) = self.width {
            let fill_w = (ratio as f32 * w).clamp(0.0, w);
            let thumb_left = ratio as f32 * w - thumb_size / 2.0;
            // Fill bar sized to fraction.
            let fill = el()
                .key(format!("{}-fill", self.key))
                .absolute(0.0, track_y)
                .w(w)
                .h(track_h)
                .child(
                    row()
                        .w(w)
                        .h(track_h)
                        .child(
                            el()
                                .w(fill_w.max(0.0))
                                .h(track_h)
                                .radius(track_radius)
                                .fill(c.bg_accent),
                        ),
                );
            // Keyed thumb element: positioned so its center sits at ratio*w.
            let thumb = el()
                .absolute(0.0, thumb_y)
                .w(w)
                .h(thumb_size)
                .child(
                    row()
                        .w(w)
                        .h(thumb_size)
                        .child(el().grow(ratio as f32).min_w(0.0).h(thumb_size))
                        .child(
                            el()
                                .key(format!("{}-thumb", self.key))
                                .w(0.0)
                                .h(thumb_size),
                        )
                        .child(el().grow((1.0 - ratio) as f32).min_w(0.0).h(thumb_size)),
                )
                .child(
                    row()
                        .absolute(0.0, 0.0)
                        .w(w)
                        .h(thumb_size)
                        .child(el().w(thumb_left.max(0.0)).h(thumb_size))
                        .child(
                            el()
                                .size(thumb_size, thumb_size)
                                .radius(thumb_size / 2.0)
                                .fill(c.knob)
                                .border(1.0, c.border_strong),
                        ),
                );
            (fill, thumb)
        } else {
            // Fallback: travel-based positioning using grow spacers.
            let fill = el()
                .key(format!("{}-fill", self.key))
                .absolute(0.0, track_y)
                .w_full()
                .h(track_h)
                .child(
                    row()
                        .w_full()
                        .h(track_h)
                        .child(
                            el()
                                .grow(ratio as f32)
                                .min_w(0.0)
                                .h(track_h)
                                .radius(track_radius)
                                .fill(c.bg_accent),
                        )
                        .child(el().grow((1.0 - ratio) as f32).min_w(0.0).h(track_h)),
                );
            let thumb = el()
                .key(format!("{}-thumb", self.key))
                .absolute(0.0, thumb_y)
                .w_full()
                .h(thumb_size)
                .child(
                    row()
                        .w_full()
                        .h(thumb_size)
                        .child(el().grow(ratio as f32).min_w(0.0).h(thumb_size))
                        .child(
                            el()
                                .size(thumb_size, thumb_size)
                                .radius(thumb_size / 2.0)
                                .fill(c.knob)
                                .border(1.0, c.border_strong),
                        )
                        .child(el().grow((1.0 - ratio) as f32).min_w(0.0).h(thumb_size)),
                );
            (fill, thumb)
        };

        let mut track = el()
            .key(self.key.clone())
            .role("slider")
            .h(hit_h)
            .child(track_bg)
            .child(fill_el)
            .child(thumb_el)
            .range_value(value, min, max)
            .range_step(step);

        if let Some(w) = self.width {
            track = track.w(w);
        } else {
            track = track.w_full();
        }

        if let Some(label) = &self.label {
            track = track.label(label.clone());
        }

        if self.disabled {
            track = track.opacity(0.5);
            // neither drags nor focuses
            return track;
        }

        track = track.focusable(true).focus_ring(2.0, 2.0, c.border_focus);

        if let Some(on_change) = self.on_change {
            let drag_change = on_change.clone();
            track = track.on_drag(move |s: &mut S, d: &mut Drag, _tx: &mut Tx| {
                if matches!(d.phase, Phase::Down | Phase::Move) {
                    let ratio = if d.w > 0.0 { (d.x / d.w) as f64 } else { 0.0 };
                    drag_change(s, snap(min, max, step, ratio));
                }
            });

            let key_change = on_change;
            let current = value;
            track = track.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                match ev.key {
                    Key::Left | Key::Down => {
                        key_change(s, (current - step).clamp(min.min(max), max.max(min)));
                        true
                    }
                    Key::Right | Key::Up => {
                        key_change(s, (current + step).clamp(min.min(max), max.max(min)));
                        true
                    }
                    Key::Home => {
                        key_change(s, min.min(max));
                        true
                    }
                    Key::End => {
                        key_change(s, max.max(min));
                        true
                    }
                    _ => false,
                }
            });
        }

        track
    }
}
