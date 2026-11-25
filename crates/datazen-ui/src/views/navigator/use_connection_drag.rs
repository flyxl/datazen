//! React `windows/connection/navigator/useConnectionDrag.ts` — reordering a
//! connection through the list, and moving one between groups, as a
//! self-contained state machine (T574).
//!
//! Split out of `ConnectionNavigatorTree` in React because it *is* a
//! self-contained state machine — the only things it needs from the navigator
//! are the current `connections` array and a couple of store writes. The
//! transitions are the point: `drag_over` is the only thing that *sets* a row
//! target, `drag_end` and the container's `drag_leave` are the only things
//! that clear one, and every handler checks the in-memory drag id so that a
//! table or view dragged in from the schema tree passes straight through to
//! the SQL editor's drop handler instead of being swallowed as a reordering.
//!
//! DOM shape: the React handlers take `DragEvent`s and answer with
//! `preventDefault` + `dropEffect`; the machine takes plain geometry and
//! answers with a [`DragFeedback`] the host translates into exactly those
//! two calls. Persistence (store moves, `saveConnection`, `reorderConnections`,
//! the cross-window ping) is emitted as [`DragEffect`]s for the host — no
//! IPC, no store, mirroring the codebase's host-port convention.
//!
//! The ghost (`createDragGhost`/`removeDragGhost`, T582's utils port) is a
//! host visual: the machine only says *when* (`ShowDragGhost` on start,
//! `HideDragGhost` on end/drop). One React quirk kept deliberately:
//! `handleGroupDrop` clears the drag state but never removes the ghost —
//! the DOM node lingered until the next drag replaced it, and the effect
//! list here says the same.

use crate::views::navigator::build_flat_rows::deps::locator::{
    PINNED_GROUP_KEY, RECENT_GROUP_KEY,
};

/// React `ConnectionDropTarget` — where a dragged connection row would land
/// if it were dropped now.
#[allow(dead_code)] // drag port payload; consumer: the navigator tree host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionDropTarget {
    pub id: String,
    /// `'before' | 'after'` — which edge of the target row.
    pub before: bool,
    /// The section/group the target row sits in; `None` is the ungrouped
    /// section. Crossing into a different group is a move as well as a
    /// reorder (see [`DragEffect::SaveConnectionGroup`]).
    pub target_group: Option<String>,
}

/// React `GroupDropTarget` (`NavigatorTreeRow.tsx`) — a drop on a group
/// header, or on a group's empty-state hint row.
#[allow(dead_code)] // drag port payload; consumer: the navigator tree host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupDropTarget {
    pub group_name: String,
    /// `'header' | 'empty'`.
    pub empty: bool,
}

/// What the host must do with the platform drag event, mapped 1:1 from
/// React's `preventDefault` / `dropEffect` pair:
///
/// * [`DragFeedback::AllowMove`] — `preventDefault()` + `dropEffect = 'move'`
///   (the position is offered for the row's drop line).
/// * [`DragFeedback::RejectNone`] — `preventDefault()` + `dropEffect = 'none'`
///   (the drag is over us but the drop is refused: recent/pinned).
/// * [`DragFeedback::Ignore`] — neither call (a foreign drag passes through
///   untouched, so the SQL editor's drop handler still sees it).
#[allow(dead_code)] // drag port; consumer: the navigator tree host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragFeedback {
    AllowMove,
    RejectNone,
    Ignore,
}

/// The read set this hook has off a connection: identity, the name the ghost
/// paints, and the group a cross-group drop would move it out of.
#[allow(dead_code)] // drag port payload; consumer: the navigator tree host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragConnectionSnapshot {
    pub id: String,
    pub name: String,
    pub group: Option<String>,
}

/// Every side effect a drag transition can ask the host for.
#[allow(dead_code)] // drag port; consumer: the navigator tree host (not wired).
#[derive(Clone, Debug, PartialEq)]
pub enum DragEffect {
    /// `dataTransfer.setData('text/plain' + 'application/datazen-connection')`
    /// — the fallback payload for drops this window did not start.
    SetDragPayload { conn_id: String },
    /// `setDragImage(createDragGhost(label), 16, 14)`.
    ShowDragGhost { label: String, offset_x: f32, offset_y: f32 },
    /// `removeDragGhost()`.
    HideDragGhost,
    /// Store `moveConnectionToGroup` (+ persist): a group drop.
    MoveConnectionToGroup { conn_id: String, group: Option<String> },
    /// `saveConnection({ ...conn, group })` — a reorder that crosses groups
    /// must persist the new bucket or the reorder alone drops it back.
    SaveConnectionGroup { conn_id: String, group: Option<String> },
    /// `connectionCommands.reorderConnections(ordered)`.
    ReorderConnections { ordered_ids: Vec<String> },
    /// `useConnectionStore.getState().fetchConnections()`.
    FetchConnections,
    /// `emitCrossWindow(EVENT_CONNECTIONS_CHANGED)` — other windows re-read.
    EmitConnectionsChanged,
}

/// The drag state machine. One per navigator tree; the host owns it and
/// feeds it the events its rows forward.
#[allow(dead_code)] // drag port; consumer: the navigator tree host (not wired).
#[derive(Default)]
pub struct ConnectionDragMachine {
    drag_conn_id: Option<String>,
    drop_target: Option<ConnectionDropTarget>,
    group_drop_target: Option<GroupDropTarget>,
}

impl ConnectionDragMachine {
    /// React `handleDragStart` — remember who is dragging, publish the
    /// payload, and show the ghost. The React text-selection clear
    /// (`window.getSelection().removeAllRanges()`) is a platform concern the
    /// host handles with its own focus/selection API.
    pub fn drag_start(
        &mut self,
        conn_id: &str,
        connections: &[DragConnectionSnapshot],
    ) -> Vec<DragEffect> {
        self.drag_conn_id = Some(conn_id.to_owned());
        let label = connections
            .iter()
            .find(|c| c.id == conn_id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| conn_id.to_owned());
        vec![
            DragEffect::SetDragPayload { conn_id: conn_id.to_owned() },
            DragEffect::ShowDragGhost { label, offset_x: 16.0, offset_y: 14.0 },
        ]
    }

    /// The in-memory drag id, if a connection drag is in flight.
    pub fn drag_conn_id(&self) -> Option<&str> {
        self.drag_conn_id.as_deref()
    }

    pub fn drop_target(&self) -> Option<&ConnectionDropTarget> {
        self.drop_target.as_ref()
    }

    pub fn group_drop_target(&self) -> Option<&GroupDropTarget> {
        self.group_drop_target.as_ref()
    }

    /// The exposed setters — the scroll container's `dragleave` has to be
    /// able to clear a target that was set by a row `dragover` it never saw.
    pub fn set_drop_target(&mut self, target: Option<ConnectionDropTarget>) {
        self.drop_target = target;
    }

    pub fn set_group_drop_target(&mut self, target: Option<GroupDropTarget>) {
        self.group_drop_target = target;
    }

    /// React `draggedConnectionId` — reading a drop's origin. The payload is
    /// unavailable mid-drag in some engines, so the in-memory id is the
    /// primary source and the transfer data is the fallback — never the other
    /// way round, or a drag that began in another window would be able to
    /// reorder this one.
    fn dragged_connection_id(&self, transfer: Option<&str>) -> Option<String> {
        self.drag_conn_id
            .clone()
            .or_else(|| transfer.map(str::to_owned))
    }

    /// React `handleDragOver(e, targetId, targetSectionGroup?)`.
    ///
    /// Geometry: `target_top`/`target_height` stand in for
    /// `e.currentTarget.getBoundingClientRect()`; the drop line flips at the
    /// row's vertical midpoint.
    pub fn drag_over(
        &mut self,
        target_id: &str,
        target_section_group: Option<&str>,
        cursor_y: f32,
        target_top: f32,
        target_height: f32,
    ) -> DragFeedback {
        let dragging = self.drag_conn_id.as_deref().is_some_and(|id| id != target_id);
        if !dragging {
            self.drop_target = None;
            return DragFeedback::Ignore;
        }
        self.group_drop_target = None;
        if target_section_group == Some(RECENT_GROUP_KEY)
            || target_section_group == Some(PINNED_GROUP_KEY)
        {
            // Recent and Pinned are computed views, not storage buckets: a
            // connection can only ever live in a real group, so offering the
            // drop here would accept a move that has nowhere to go.
            self.drop_target = None;
            return DragFeedback::RejectNone;
        }
        let mid_y = target_top + target_height / 2.0;
        let before = cursor_y < mid_y;
        let next = ConnectionDropTarget {
            id: target_id.to_owned(),
            before,
            target_group: target_section_group.map(str::to_owned),
        };
        // Keep the previous object when nothing changed: a new one would
        // re-render every row that reads the target on every drag event.
        if self.drop_target.as_ref() != Some(&next) {
            self.drop_target = Some(next);
        }
        DragFeedback::AllowMove
    }

    /// React `handleDragLeave` — deliberately empty. In WebKit/Safari
    /// `relatedTarget` is always null during drag events, so clearing the
    /// target on row dragleave would wipe it on every mouse move across
    /// children. The target is updated by every dragover and cleared by
    /// dragend or the container's dragleave.
    pub fn drag_leave_row(&mut self) {}

    /// React `handleGroupDragLeave` — same rule as [`Self::drag_leave_row`]:
    /// do not clear on child leave; dragover updates, dragend clears.
    pub fn group_drag_leave(&mut self) {}

    /// React `handleDragEnd` — the one place a drag fully unwinds.
    pub fn drag_end(&mut self) -> Vec<DragEffect> {
        self.drag_conn_id = None;
        self.drop_target = None;
        self.group_drop_target = None;
        vec![DragEffect::HideDragGhost]
    }

    /// React `handleGroupDragOver(e, groupName, target = 'header')`.
    pub fn group_drag_over(
        &mut self,
        group_name: &str,
        empty: bool,
    ) -> DragFeedback {
        if self.drag_conn_id.is_none() {
            return DragFeedback::Ignore;
        }
        self.drop_target = None;
        let next = GroupDropTarget {
            group_name: group_name.to_owned(),
            empty,
        };
        if self.group_drop_target.as_ref() != Some(&next) {
            self.group_drop_target = Some(next);
        }
        DragFeedback::AllowMove
    }

    /// React `handleGroupDrop(e, groupName)` — only consume drops from
    /// connection reordering drags; table/view drops must pass through to
    /// the SQL editor drop handler. Returns `None` (and touches nothing)
    /// when the drop is not ours.
    pub fn group_drop(
        &mut self,
        transfer_conn_id: Option<&str>,
        group_name: &str,
        connections: &[DragConnectionSnapshot],
    ) -> Option<Vec<DragEffect>> {
        let conn_id = self.dragged_connection_id(transfer_conn_id)?;
        if !connections.iter().any(|c| c.id == conn_id) {
            return None;
        }
        // React `groupName || undefined`: an empty group name means ungrouped.
        let group = if group_name.is_empty() { None } else { Some(group_name.to_owned()) };
        self.drag_conn_id = None;
        self.drop_target = None;
        self.group_drop_target = None;
        // Deliberately no `HideDragGhost` here — see the module doc.
        Some(vec![
            DragEffect::MoveConnectionToGroup { conn_id, group },
            DragEffect::FetchConnections,
            DragEffect::EmitConnectionsChanged,
        ])
    }

    /// React `handleSectionDragOver(e, section)` — the pinned/recent section
    /// headers swallow the drag so nothing below them can accept it.
    pub fn section_drag_over(&mut self, section: &str) -> DragFeedback {
        if self.drag_conn_id.is_none() {
            return DragFeedback::Ignore;
        }
        if section == "recent" || section == "pinned" {
            return DragFeedback::RejectNone;
        }
        DragFeedback::Ignore
    }

    /// React `handleSectionDrop(e, _section)` — consume the drop (it landed
    /// in a section, not on a row or group) and unwind the drag markers.
    pub fn section_drop(&mut self) -> Option<Vec<DragEffect>> {
        if self.drag_conn_id.is_none() {
            return None;
        }
        self.drop_target = None;
        self.group_drop_target = None;
        Some(Vec::new())
    }

    /// React `handleDrop(e)` — the reorder itself.
    ///
    /// Returns `None` when the drop is not ours to consume; `Some(effects)`
    /// otherwise (possibly empty: a no-op reorder still unwinds the drag).
    pub fn drop(
        &mut self,
        transfer_conn_id: Option<&str>,
        connections: &[DragConnectionSnapshot],
    ) -> Option<Vec<DragEffect>> {
        let source_id = self.dragged_connection_id(transfer_conn_id)?;
        let Some(target) = self.drop_target.clone() else {
            // Ours, but nothing to land on: the drag still unwinds
            // (React calls `handleDragEnd`, ghost included).
            return Some(self.drag_end());
        };

        let ids: Vec<&str> = connections.iter().map(|c| c.id.as_str()).collect();
        let from_index = ids.iter().position(|id| *id == source_id);
        let to_index = ids.iter().position(|id| *id == target.id);
        let (from_index, to_index) = match (from_index, to_index) {
            (Some(from), Some(to)) if from != to => (from, to),
            // Unknown ids or a drop on itself: nothing to reorder, but the
            // drag is still ours and unwinds exactly the same way.
            _ => return Some(self.drag_end()),
        };

        // The splice: lift the source out, then re-insert before or after the
        // target *in the array that no longer contains the source* — that is
        // what makes `before`/`after` behave when the source sat below.
        let mut reordered: Vec<String> = ids.into_iter().map(str::to_owned).collect();
        reordered.remove(from_index);
        let target_pos = reordered
            .iter()
            .position(|id| *id == target.id)
            .unwrap_or(0);
        let insert_at = if target.before { target_pos } else { target_pos + 1 };
        reordered.insert(insert_at, source_id.clone());

        let source_conn = connections.iter().find(|c| c.id == source_id);
        let mut effects = Vec::new();
        // Crossing into a different group is a *move* as well as a reorder:
        // the persisted `group` is the only thing that puts it in the new
        // bucket, so the reorder alone would silently drop it back.
        if let Some(conn) = source_conn {
            let target_group = target.target_group.clone();
            let current = conn.group.clone().unwrap_or_default();
            if target_group.is_some() && current != target_group.clone().unwrap_or_default() {
                effects.push(DragEffect::SaveConnectionGroup {
                    conn_id: source_id.clone(),
                    // React `targetGroup || undefined`: '' means ungrouped.
                    group: target_group.filter(|g| !g.is_empty()),
                });
            }
        }
        effects.push(DragEffect::ReorderConnections { ordered_ids: reordered });
        effects.push(DragEffect::FetchConnections);
        effects.push(DragEffect::EmitConnectionsChanged);

        // Then the drag fully unwinds, ghost included (React's closing
        // `handleDragEnd()` — `HideDragGhost` lands last, after the writes).
        effects.extend(self.drag_end());
        Some(effects)
    }
}
