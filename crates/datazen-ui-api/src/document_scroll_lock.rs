#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollPosition {
    pub root_top: i64,
    pub root_left: i64,
    pub body_present: bool,
    pub body_top: i64,
    pub body_left: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    RootTop,
    RootLeft,
    BodyTop,
    BodyLeft,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollLockState {
    pub installed: bool,
    pub last_root_top: i64,
    pub last_body_top: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollLockRail {
    Unavailable,
}

impl ScrollLockRail {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
        }
    }
}

pub fn scroll_lock_rail() -> ScrollLockRail {
    ScrollLockRail::Unavailable
}

pub fn snap_targets(position: ScrollPosition) -> Vec<ScrollAxis> {
    let mut targets = Vec::new();
    if position.root_top != 0 {
        targets.push(ScrollAxis::RootTop);
    }
    if position.root_left != 0 {
        targets.push(ScrollAxis::RootLeft);
    }
    if position.body_present {
        if position.body_top != 0 {
            targets.push(ScrollAxis::BodyTop);
        }
        if position.body_left != 0 {
            targets.push(ScrollAxis::BodyLeft);
        }
    }
    targets
}

pub fn apply_snap(position: &mut ScrollPosition) -> Vec<ScrollAxis> {
    let targets = snap_targets(*position);
    for target in &targets {
        match target {
            ScrollAxis::RootTop => position.root_top = 0,
            ScrollAxis::RootLeft => position.root_left = 0,
            ScrollAxis::BodyTop => position.body_top = 0,
            ScrollAxis::BodyLeft => position.body_left = 0,
        }
    }
    targets
}

impl ScrollLockState {
    pub fn install(&mut self) -> bool {
        if self.installed {
            return false;
        }
        self.installed = true;
        self.last_root_top = 0;
        self.last_body_top = 0;
        true
    }

    pub fn poll_changed(&mut self, current: ScrollPosition) -> bool {
        let body_top = if current.body_present { current.body_top } else { 0 };
        let changed = current.root_top != self.last_root_top || body_top != self.last_body_top;
        if changed {
            self.last_root_top = current.root_top;
            self.last_body_top = body_top;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shifted() -> ScrollPosition {
        ScrollPosition {
            root_top: 40,
            root_left: 12,
            body_present: true,
            body_top: 9,
            body_left: 3,
        }
    }

    #[test]
    fn a_position_at_the_origin_needs_no_snap() {
        let position = ScrollPosition::default();
        assert!(snap_targets(position).is_empty());
        assert!(apply_snap(&mut ScrollPosition::default()).is_empty());
    }

    #[test]
    fn every_nonzero_axis_is_reported_in_root_body_order() {
        assert_eq!(
            snap_targets(shifted()),
            vec![
                ScrollAxis::RootTop,
                ScrollAxis::RootLeft,
                ScrollAxis::BodyTop,
                ScrollAxis::BodyLeft,
            ]
        );
    }

    #[test]
    fn applying_a_snap_resets_only_the_reported_axes() {
        let mut position = shifted();
        position.body_left = 0;
        assert_eq!(
            apply_snap(&mut position),
            vec![ScrollAxis::RootTop, ScrollAxis::RootLeft, ScrollAxis::BodyTop]
        );
        assert_eq!(position, ScrollPosition { body_present: true, ..Default::default() });
    }

    #[test]
    fn an_absent_body_is_never_snapped() {
        let mut position = shifted();
        position.body_present = false;
        assert_eq!(
            snap_targets(position),
            vec![ScrollAxis::RootTop, ScrollAxis::RootLeft]
        );
        assert_eq!(
            apply_snap(&mut position),
            vec![ScrollAxis::RootTop, ScrollAxis::RootLeft]
        );
    }

    #[test]
    fn install_is_idempotent_and_does_not_reinitialize_once_installed() {
        let mut state = ScrollLockState::default();
        assert!(state.install());
        assert_eq!(state, ScrollLockState { installed: true, ..Default::default() });
        state.last_root_top = 7;
        assert!(!state.install());
        assert_eq!(state.last_root_top, 7);
    }

    #[test]
    fn polling_watches_only_vertical_offsets() {
        let mut state = ScrollLockState::default();
        assert!(state.install());
        let mut position = ScrollPosition::default();
        position.root_left = 30;
        position.body_left = 20;
        assert!(!state.poll_changed(position));
        position.root_top = 5;
        assert!(state.poll_changed(position));
        assert_eq!(state.last_root_top, 5);
        assert_eq!(state.last_body_top, 0);
        assert!(!state.poll_changed(position));
    }

    #[test]
    fn polling_uses_zero_for_an_absent_body() {
        let mut state = ScrollLockState::default();
        assert!(state.install());
        let mut position = ScrollPosition { body_present: false, body_top: 99, ..Default::default() };
        assert!(!state.poll_changed(position));
        position.root_top = 1;
        assert!(state.poll_changed(position));
        assert_eq!(state.last_body_top, 0);
    }

    #[test]
    fn the_dom_scroll_lock_rail_is_reported_as_unavailable() {
        assert_eq!(scroll_lock_rail(), ScrollLockRail::Unavailable);
        assert_eq!(scroll_lock_rail().as_str(), "unavailable");
    }
}
