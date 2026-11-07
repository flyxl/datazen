import { useCallback, useRef, useState } from 'react';
import type { DragEvent } from 'react';
import { useConnectionStore } from '../../../stores/connectionStore';
import { connectionCommands } from '../../../commands/connection';
import { emitCrossWindow } from '../../../lib/crossWindowBus';
import { EVENT_CONNECTIONS_CHANGED } from '../../../stores/connectionStore';
import { PINNED_GROUP_KEY, RECENT_GROUP_KEY } from '../../../lib/connectionLocator';
import { createDragGhost, removeDragGhost } from './utils';
import type { GroupDropTarget } from './NavigatorTreeRow';

/** Where a dragged connection row would land if it were dropped now. */
export type ConnectionDropTarget = {
  id: string;
  position: 'before' | 'after';
  targetGroup?: string;
};

/** The drag payload every one of these handlers answers to. */
const DRAG_MIME = 'application/datazen-connection';

/**
 * Reading a drop's origin. The payload is unavailable mid-drag in some engines,
 * so the in-memory `dragConnId` is the primary source and the transfer data is
 * the fallback — never the other way round, or a drag that began in another
 * window would be able to reorder this one.
 */
function draggedConnectionId(e: DragEvent, fallback: string | null): string | null {
  const dt = e.dataTransfer;
  const fromTransfer =
    typeof dt?.getData === 'function' ? dt.getData(DRAG_MIME) || dt.getData('text/plain') : null;
  return fallback || fromTransfer || null;
}

/**
 * Reordering a connection through the list, and moving one between groups.
 *
 * Split out of `ConnectionNavigatorTree` because it is a self-contained state
 * machine — the only things it needs from the navigator are the current
 * `connections` array and a couple of store writes. It reads as a unit here
 * because the transitions are the point: `dragover` is the only thing that
 * *sets* a target, `dragend` and the container's `dragleave` are the only things
 * that clear one, and every handler checks `dragConnId` so that a table or view
 * dragged in from the schema tree passes straight through to the SQL editor's
 * drop handler instead of being swallowed as a reordering.
 */
export function useConnectionDrag(
  connections: ReturnType<typeof useConnectionStore.getState>['connections'],
) {
  const dragConnId = useRef<string | null>(null);
  const [dropTarget, setDropTarget] = useState<ConnectionDropTarget | null>(null);
  const [groupDropTarget, setGroupDropTarget] = useState<GroupDropTarget | null>(null);
  // Read inside `handleDrop`, which is registered as a DOM listener and must
  // not re-subscribe on every pointer move.
  const dropTargetRef = useRef(dropTarget);
  dropTargetRef.current = dropTarget;

  const handleDragStart = useCallback(
    (e: DragEvent, connId: string) => {
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed) {
        sel.removeAllRanges();
      }

      dragConnId.current = connId;
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', connId);
      e.dataTransfer.setData(DRAG_MIME, connId);

      if (e.dataTransfer.setDragImage) {
        const conn = connections.find((c) => c.id === connId);
        e.dataTransfer.setDragImage(createDragGhost(conn?.name ?? connId), 16, 14);
      }
    },
    [connections],
  );

  const handleDragOver = useCallback(
    (e: DragEvent, targetId: string, targetSectionGroup?: string) => {
      if (!dragConnId.current || dragConnId.current === targetId) {
        setDropTarget(null);
        return;
      }
      setGroupDropTarget(null);
      if (targetSectionGroup === RECENT_GROUP_KEY || targetSectionGroup === PINNED_GROUP_KEY) {
        // Recent and Pinned are computed views, not storage buckets: a connection
        // can only ever live in a real group, so offering the drop here would
        // accept a move that has nowhere to go.
        e.preventDefault();
        e.dataTransfer.dropEffect = 'none';
        setDropTarget(null);
        return;
      }
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      const rect = e.currentTarget.getBoundingClientRect();
      const midY = rect.top + rect.height / 2;
      const position = e.clientY < midY ? 'before' : 'after';
      // Keep the previous object when nothing changed: a new one would re-render
      // every row that reads `dropTarget` on every single drag event.
      setDropTarget((prev) =>
        prev?.id === targetId &&
        prev.position === position &&
        prev.targetGroup === targetSectionGroup
          ? prev
          : { id: targetId, position, targetGroup: targetSectionGroup },
      );
    },
    [],
  );

  const handleDragLeave = useCallback((_e: DragEvent) => {
    // In WebKit / Safari, e.relatedTarget is ALWAYS null during drag events.
    // Relying on relatedTarget to check if drag left the element will ALWAYS evaluate to false
    // and wipe dropTarget on every mouse move across children!
    // Therefore, do NOT clear dropTarget on row dragleave.
    // dropTarget is updated on every dragover, and cleared on dragend or container dragleave.
  }, []);

  const handleDragEnd = useCallback(() => {
    dragConnId.current = null;
    setDropTarget(null);
    setGroupDropTarget(null);
    removeDragGhost();
  }, []);

  const handleGroupDragOver = useCallback(
    (e: DragEvent, groupName: string, target: 'header' | 'empty' = 'header') => {
      if (!dragConnId.current) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      setDropTarget(null);
      setGroupDropTarget((prev) =>
        prev?.groupName === groupName && prev.target === target ? prev : { groupName, target },
      );
    },
    [],
  );

  const handleGroupDragLeave = useCallback((_e: DragEvent) => {
    // Do not clear groupDropTarget on child leave; it will be updated by dragover
    // or cleared on dragend.
  }, []);

  const handleGroupDrop = useCallback(
    (e: DragEvent, groupName: string) => {
      // Only consume drops from connection reordering drags — table/view drops
      // must pass through to the SQL editor drop handler.
      const connId = draggedConnectionId(e, dragConnId.current);
      if (!connId || !connections.some((c) => c.id === connId)) return;
      e.preventDefault();
      void (async () => {
        await useConnectionStore.getState().moveConnectionToGroup(connId, groupName || undefined);
        await useConnectionStore.getState().fetchConnections?.();
        void emitCrossWindow(EVENT_CONNECTIONS_CHANGED);
      })();
      dragConnId.current = null;
      setDropTarget(null);
      setGroupDropTarget(null);
    },
    [connections],
  );

  const handleSectionDragOver = useCallback((e: DragEvent, section: string) => {
    // Only respond to connection reordering drags — table/view drops from the
    // schema tree must pass through to the SQL editor drop handler.
    if (!dragConnId.current) return;
    if (section === 'recent' || section === 'pinned') {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'none';
    }
  }, []);

  const handleSectionDrop = useCallback((e: DragEvent, _section: string) => {
    // Only consume drops from connection reordering drags — table/view drops
    // must pass through to the SQL editor drop handler.
    if (!dragConnId.current) return;
    e.preventDefault();
    setDropTarget(null);
    setGroupDropTarget(null);
  }, []);

  const handleDrop = useCallback(
    (e: DragEvent) => {
      e.preventDefault();
      const sourceId = draggedConnectionId(e, dragConnId.current);
      const target = dropTargetRef.current;
      if (!sourceId || !target) {
        handleDragEnd();
        return;
      }

      const ids = connections.map((c) => c.id);
      const fromIndex = ids.indexOf(sourceId);
      const toIndex = ids.indexOf(target.id);
      if (fromIndex === -1 || toIndex === -1 || fromIndex === toIndex) {
        handleDragEnd();
        return;
      }

      const sourceConn = connections.find((c) => c.id === sourceId);
      const targetGroup = target.targetGroup;

      const reordered = [...ids];
      reordered.splice(fromIndex, 1);
      const insertAt =
        target.position === 'before'
          ? reordered.indexOf(target.id)
          : reordered.indexOf(target.id) + 1;
      reordered.splice(insertAt, 0, sourceId);

      void (async () => {
        // Crossing into a different group is a *move* as well as a reorder: the
        // persisted `group` is the only thing that puts it in the new bucket,
        // so the reorder alone would silently drop it back.
        if (sourceConn && targetGroup !== undefined && (sourceConn.group ?? '') !== targetGroup) {
          await connectionCommands.saveConnection({
            ...sourceConn,
            group: targetGroup || undefined,
          });
        }
        await connectionCommands.reorderConnections(reordered);
        await useConnectionStore.getState().fetchConnections();
        void emitCrossWindow(EVENT_CONNECTIONS_CHANGED);
      })();

      handleDragEnd();
    },
    [connections, handleDragEnd],
  );

  return {
    dropTarget,
    // The scroll container's `dragleave` has to be able to clear a target that
    // was set by a row `dragover` it never saw, so the setters escape too.
    setDropTarget,
    setGroupDropTarget,
    groupDropTarget,
    dropTargetRef,
    handleDragStart,
    handleDragOver,
    handleDragLeave,
    handleDragEnd,
    handleDrop,
    handleGroupDragOver,
    handleGroupDragLeave,
    handleGroupDrop,
    handleSectionDragOver,
    handleSectionDrop,
  };
}
