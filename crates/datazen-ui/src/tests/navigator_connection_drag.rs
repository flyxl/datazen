//! T574: `navigator/useConnectionDrag.ts` port — the connection reorder /
//! regroup drag state machine. No React suite pins this hook directly (its
//! behaviour is covered end-to-end by `ConnectionNavigatorTree.test.tsx`'s
//! drag blocks); the tests here pin the transitions the React file's own
//! comments call out, in the machine's vocabulary.

use crate::views::navigator::build_flat_rows::deps::locator::{
    PINNED_GROUP_KEY, RECENT_GROUP_KEY,
};
use crate::views::navigator::use_connection_drag::{
    ConnectionDragMachine, ConnectionDropTarget, DragConnectionSnapshot, DragEffect,
    DragFeedback, GroupDropTarget,
};

fn conn(id: &str, group: Option<&str>) -> DragConnectionSnapshot {
    DragConnectionSnapshot {
        id: id.to_owned(),
        name: format!("Conn {id}"),
        group: group.map(str::to_owned),
    }
}

fn conns() -> Vec<DragConnectionSnapshot> {
    vec![
        conn("c1", Some("team-a")),
        conn("c2", Some("team-a")),
        conn("c3", None),
        conn("c4", Some("team-b")),
    ]
}

/// `a` over `b`: drag a, hover b's top half.
fn drag_a_over_top_of_b() -> ConnectionDragMachine {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let _ = m.drag_over("c2", Some("team-a"), 10.0, 0.0, 28.0);
    m
}

// ---------------------------------------------------------------------------
// drag start / end
// ---------------------------------------------------------------------------

#[test]
fn drag_start_publishes_the_payload_and_shows_the_named_ghost() {
    let mut m = ConnectionDragMachine::default();
    let effects = m.drag_start("c2", &conns());
    assert_eq!(
        effects,
        vec![
            DragEffect::SetDragPayload { conn_id: "c2".to_owned() },
            DragEffect::ShowDragGhost {
                label: "Conn c2".to_owned(),
                offset_x: 16.0,
                offset_y: 14.0,
            },
        ]
    );
    assert_eq!(m.drag_conn_id(), Some("c2"));
}

#[test]
fn drag_start_falls_back_to_the_id_when_the_connection_is_unknown() {
    let mut m = ConnectionDragMachine::default();
    let effects = m.drag_start("gone", &conns());
    assert_eq!(
        effects[1],
        DragEffect::ShowDragGhost {
            label: "gone".to_owned(),
            offset_x: 16.0,
            offset_y: 14.0,
        }
    );
}

#[test]
fn drag_end_clears_everything_and_hides_the_ghost() {
    let mut m = drag_a_over_top_of_b();
    assert!(m.drop_target().is_some());
    let effects = m.drag_end();
    assert_eq!(effects, vec![DragEffect::HideDragGhost]);
    assert_eq!(m.drag_conn_id(), None);
    assert!(m.drop_target().is_none());
    assert!(m.group_drop_target().is_none());
}

// ---------------------------------------------------------------------------
// row dragover: the only thing that sets a row target
// ---------------------------------------------------------------------------

#[test]
fn drag_over_above_the_midline_targets_before_and_below_targets_after() {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    // Row [0, 28): midline 14.
    assert_eq!(m.drag_over("c2", Some("team-a"), 5.0, 0.0, 28.0), DragFeedback::AllowMove);
    assert_eq!(
        m.drop_target(),
        Some(&ConnectionDropTarget {
            id: "c2".to_owned(),
            before: true,
            target_group: Some("team-a".to_owned()),
        })
    );
    assert_eq!(m.drag_over("c2", Some("team-a"), 14.0, 0.0, 28.0), DragFeedback::AllowMove);
    assert_eq!(m.drop_target().unwrap().before, false);
}

#[test]
fn drag_over_keeps_the_previous_object_when_nothing_changed() {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let _ = m.drag_over("c2", Some("team-a"), 5.0, 0.0, 28.0);
    let before = m.drop_target().unwrap().clone();
    let _ = m.drag_over("c2", Some("team-a"), 6.0, 0.0, 28.0);
    // Same target: the machine must not manufacture a new one (React's
    // functional-update dedupe — a new object re-renders every reader).
    assert_eq!(m.drop_target(), Some(&before));
}

#[test]
fn a_row_drag_over_clears_the_group_target() {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let _ = m.group_drag_over("team-b", false);
    assert!(m.group_drop_target().is_some());
    let _ = m.drag_over("c2", Some("team-a"), 5.0, 0.0, 28.0);
    assert!(m.group_drop_target().is_none());
}

#[test]
fn drag_over_the_dragged_row_itself_is_ignored_and_clears_the_target() {
    let mut m = drag_a_over_top_of_b();
    assert!(m.drop_target().is_some());
    assert_eq!(m.drag_over("c1", Some("team-a"), 5.0, 0.0, 28.0), DragFeedback::Ignore);
    assert!(m.drop_target().is_none());
    // And without a drag at all, nothing is ever offered.
    let mut idle = ConnectionDragMachine::default();
    assert_eq!(idle.drag_over("c2", Some("team-a"), 5.0, 0.0, 28.0), DragFeedback::Ignore);
    assert!(idle.drop_target().is_none());
}

#[test]
fn the_reserved_sections_refuse_the_drop_without_clearing_the_group_target_first() {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    // Recent: a computed view, not a storage bucket — accept the event but
    // refuse the drop.
    assert_eq!(m.drag_over("c2", Some(RECENT_GROUP_KEY), 5.0, 0.0, 28.0), DragFeedback::RejectNone);
    assert!(m.drop_target().is_none());
    // Pinned: same. The group target is cleared before the check, exactly as
    // the React handler orders it.
    let _ = m.group_drag_over("team-b", false);
    assert_eq!(m.drag_over("c2", Some(PINNED_GROUP_KEY), 5.0, 0.0, 28.0), DragFeedback::RejectNone);
    assert!(m.group_drop_target().is_none());
    assert!(m.drop_target().is_none());
}

#[test]
fn row_dragleave_and_group_dragleave_are_deliberate_no_ops() {
    // WebKit: `relatedTarget` is always null mid-drag, so a child leave would
    // wipe the target on every move. Only dragend / container leave clear.
    let mut m = drag_a_over_top_of_b();
    let target = m.drop_target().unwrap().clone();
    m.drag_leave_row();
    assert_eq!(m.drop_target(), Some(&target));
    m.group_drag_leave();
    assert_eq!(m.drop_target(), Some(&target));
}

// ---------------------------------------------------------------------------
// group dragover / drop
// ---------------------------------------------------------------------------

#[test]
fn group_drag_over_sets_the_group_target_and_clears_the_row_target() {
    let mut m = drag_a_over_top_of_b();
    assert!(m.drop_target().is_some());
    assert_eq!(m.group_drag_over("team-b", false), DragFeedback::AllowMove);
    assert!(m.drop_target().is_none());
    assert_eq!(
        m.group_drop_target(),
        Some(&GroupDropTarget {
            group_name: "team-b".to_owned(),
            empty: false,
        })
    );
    // Header and empty hint are different targets; nothing changed for the
    // same (group, part) pair.
    assert_eq!(m.group_drag_over("team-b", true), DragFeedback::AllowMove);
    assert_eq!(m.group_drop_target().unwrap().empty, true);
    assert_eq!(m.group_drag_over("team-b", true), DragFeedback::AllowMove);

    // Without a connection drag in flight, a group header is not a target.
    let mut idle = ConnectionDragMachine::default();
    assert_eq!(idle.group_drag_over("team-b", false), DragFeedback::Ignore);
    assert!(idle.group_drop_target().is_none());
}

#[test]
fn a_group_drop_moves_the_connection_and_refreshes_every_window() {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c3", &conns());
    let _ = m.group_drag_over("team-b", false);
    let effects = m.group_drop(None, "team-b", &conns()).expect("our drop");
    assert_eq!(
        effects,
        vec![
            DragEffect::MoveConnectionToGroup {
                conn_id: "c3".to_owned(),
                group: Some("team-b".to_owned()),
            },
            DragEffect::FetchConnections,
            DragEffect::EmitConnectionsChanged,
        ]
    );
    // The drag is fully consumed: no id, no targets, no ghost request (the
    // React handler never removes the ghost here — kept deliberately).
    assert_eq!(m.drag_conn_id(), None);
    assert!(m.drop_target().is_none());
    assert!(m.group_drop_target().is_none());
}

#[test]
fn a_group_drop_on_an_empty_group_name_moves_to_ungrouped() {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let effects = m.group_drop(None, "", &conns()).expect("our drop");
    assert_eq!(
        effects[0],
        DragEffect::MoveConnectionToGroup { conn_id: "c1".to_owned(), group: None }
    );
}

#[test]
fn a_foreign_drop_on_a_group_passes_through_untouched() {
    let mut m = ConnectionDragMachine::default();
    // A table drag from the schema tree: no in-memory id, transfer says
    // nothing we know.
    assert!(m.group_drop(None, "team-b", &conns()).is_none());
    assert!(m.group_drop(Some("public.users"), "team-b", &conns()).is_none());
    // And a drop naming a connection that no longer exists is just as
    // foreign: the store write would resurrect it.
    assert!(m.group_drop(Some("gone"), "team-b", &conns()).is_none());
    assert!(m.group_drop(None, "team-b", &conns()).is_none());
}

#[test]
fn the_transfer_payload_only_counts_when_it_names_a_live_connection() {
    // Mid-drag the in-memory id is primary — a drag that began in another
    // window (transfer only) may reorder this one's list only if it names a
    // connection this window still holds.
    let mut m = ConnectionDragMachine::default();
    let effects = m.group_drop(Some("c4"), "team-a", &conns()).expect("foreign but live");
    assert_eq!(
        effects[0],
        DragEffect::MoveConnectionToGroup { conn_id: "c4".to_owned(), group: Some("team-a".to_owned()) }
    );
}

#[test]
fn the_in_memory_id_wins_when_the_transfer_disagrees() {
    // The payload is unavailable mid-drag in some engines, so the in-memory
    // id is primary and the transfer data is the fallback — never the other
    // way round, or a drag that began in another window would be able to
    // reorder this one under a stale payload.
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let effects = m
        .group_drop(Some("c2"), "team-b", &conns())
        .expect("our drop");
    assert_eq!(
        effects[0],
        DragEffect::MoveConnectionToGroup { conn_id: "c1".to_owned(), group: Some("team-b".to_owned()) }
    );

    // Same rule on the container drop: the reorder moves the dragged row,
    // not whatever the transfer claims.
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let _ = m.drag_over("c3", None, 20.0, 0.0, 28.0); // after
    let effects = m.drop(Some("c4"), &conns()).expect("our drop");
    assert!(matches!(
        &effects[0],
        DragEffect::ReorderConnections { ordered_ids }
            if ordered_ids == &["c2", "c3", "c1", "c4"].map(|s| s.to_owned()).to_vec()
    ));
}

// ---------------------------------------------------------------------------
// section dragover / drop
// ---------------------------------------------------------------------------

#[test]
fn the_pinned_and_recent_section_headers_swallow_the_drag() {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    assert_eq!(m.section_drag_over("pinned"), DragFeedback::RejectNone);
    assert_eq!(m.section_drag_over("recent"), DragFeedback::RejectNone);
    // A user-named section is not a drop target at all.
    assert_eq!(m.section_drag_over("team-a"), DragFeedback::Ignore);
    // Without a drag, nothing is swallowed — foreign drops pass through.
    let mut idle = ConnectionDragMachine::default();
    assert_eq!(idle.section_drag_over("pinned"), DragFeedback::Ignore);
}

#[test]
fn a_section_drop_unwinds_the_markers_without_reordering() {
    let mut m = drag_a_over_top_of_b();
    let effects = m.section_drop().expect("our drop");
    assert!(effects.is_empty());
    assert!(m.drop_target().is_none());
    assert!(m.group_drop_target().is_none());
    assert_eq!(m.drag_conn_id(), Some("c1"));
    // Without a connection drag, a section drop is not ours to consume.
    let mut idle = ConnectionDragMachine::default();
    assert!(idle.section_drop().is_none());
}

// ---------------------------------------------------------------------------
// the drop: reorder math and cross-group persistence
// ---------------------------------------------------------------------------

#[test]
fn dropping_before_and_after_reorders_relative_to_the_reduced_list() {
    // [c1, c2, c3, c4]: drop c1 after c3. Remove c1 -> [c2, c3, c4]; c3 now
    // sits at 1; after inserts at 2 -> [c2, c3, c1, c4].
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let _ = m.drag_over("c3", None, 20.0, 0.0, 28.0); // below midline: after
    let effects = m.drop(None, &conns()).expect("our drop");
    assert_eq!(
        effects,
        vec![
            DragEffect::ReorderConnections {
                ordered_ids: vec![
                    "c2".to_owned(),
                    "c3".to_owned(),
                    "c1".to_owned(),
                    "c4".to_owned(),
                ],
            },
            DragEffect::FetchConnections,
            DragEffect::EmitConnectionsChanged,
            DragEffect::HideDragGhost,
        ]
    );

    // Drop c4 before c3 (both outside the group ladder the target reports:
    // a pure reorder, no group persist).
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c4", &conns());
    let _ = m.drag_over("c3", None, 0.0, 0.0, 28.0); // above midline: before
    let effects = m.drop(None, &conns()).expect("our drop");
    assert_eq!(
        effects[0],
        DragEffect::ReorderConnections {
            ordered_ids: vec![
                "c1".to_owned(),
                "c2".to_owned(),
                "c4".to_owned(),
                "c3".to_owned(),
            ],
        }
    );
    // And a same-group pair for the third shape: c2 after c1 keeps the order
    // but exercises the reduced-list insert path.
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c2", &conns());
    let _ = m.drag_over("c1", Some("team-a"), 27.0, 0.0, 28.0); // below: after
    let effects = m.drop(None, &conns()).expect("our drop");
    assert_eq!(
        effects[0],
        DragEffect::ReorderConnections {
            ordered_ids: vec![
                "c1".to_owned(),
                "c2".to_owned(),
                "c3".to_owned(),
                "c4".to_owned(),
            ],
        }
    );
}

#[test]
fn crossing_into_another_group_persists_the_group_before_the_reorder() {
    // c3 is ungrouped, the target row sits in team-b: the persisted group is
    // the only thing that puts it in the new bucket.
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c3", &conns());
    let _ = m.drag_over("c4", Some("team-b"), 0.0, 0.0, 28.0);
    let effects = m.drop(None, &conns()).expect("our drop");
    assert_eq!(
        effects[0],
        DragEffect::SaveConnectionGroup {
            conn_id: "c3".to_owned(),
            group: Some("team-b".to_owned()),
        }
    );
    assert!(matches!(effects[1], DragEffect::ReorderConnections { .. }));
}

#[test]
fn dropping_into_the_same_group_does_not_persist_a_group() {
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let _ = m.drag_over("c2", Some("team-a"), 0.0, 0.0, 28.0);
    let effects = m.drop(None, &conns()).expect("our drop");
    assert!(matches!(effects[0], DragEffect::ReorderConnections { .. }));

    // An ungrouped row dropped on the ungrouped section: no move either.
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c3", &conns());
    let _ = m.drag_over("c3x", None, 0.0, 0.0, 28.0);
    // (c3 on itself is refused; use a second ungrouped row instead.)
    let conns2 = vec![conn("c3", None), conn("c5", None)];
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c3", &conns2);
    let _ = m.drag_over("c5", None, 0.0, 0.0, 28.0);
    let effects = m.drop(None, &conns2).expect("our drop");
    assert!(matches!(effects[0], DragEffect::ReorderConnections { .. }));
}

#[test]
fn a_cross_group_drop_onto_the_ungrouped_section_clears_the_group() {
    let conns2 = vec![conn("c1", Some("team-a")), conn("c2", None)];
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns2);
    // The ungrouped section reports targetGroup "" (empty string in React).
    let _ = m.drag_over("c2", Some(""), 0.0, 0.0, 28.0);
    let effects = m.drop(None, &conns2).expect("our drop");
    assert_eq!(
        effects[0],
        DragEffect::SaveConnectionGroup { conn_id: "c1".to_owned(), group: None }
    );
}

#[test]
fn a_drop_with_no_target_or_a_noop_reorder_still_unwinds_the_drag() {
    // Target never set (no dragover): the drag unwinds, ghost hidden.
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let effects = m.drop(None, &conns()).expect("our drop");
    assert_eq!(effects, vec![DragEffect::HideDragGhost]);
    assert_eq!(m.drag_conn_id(), None);

    // Source id vanished from the list mid-drag: same unwind.
    let mut m = ConnectionDragMachine::default();
    let _ = m.drag_start("c1", &conns());
    let _ = m.drag_over("c2", Some("team-a"), 0.0, 0.0, 28.0);
    let shrunk = vec![conn("c2", Some("team-a")), conn("c3", None)];
    let effects = m.drop(None, &shrunk).expect("our drop");
    assert_eq!(effects, vec![DragEffect::HideDragGhost]);

    // A foreign drop that reaches the container is still consumed — React
    // preventDefaults first thing and unwinds the (empty) drag — but the
    // reorder never runs and nothing persists.
    let mut m = ConnectionDragMachine::default();
    let effects = m.drop(Some("public.users"), &conns()).expect("container consumes");
    assert_eq!(effects, vec![DragEffect::HideDragGhost]);
    assert_eq!(m.drag_conn_id(), None);
}

// ---------------------------------------------------------------------------
// the exposed setters (the container's dragleave)
// ---------------------------------------------------------------------------

#[test]
fn the_container_may_clear_a_target_it_never_saw_set() {
    let mut m = drag_a_over_top_of_b();
    m.set_drop_target(None);
    assert!(m.drop_target().is_none());
    let _ = m.group_drag_over("team-b", false);
    m.set_group_drop_target(None);
    assert!(m.group_drop_target().is_none());
}
