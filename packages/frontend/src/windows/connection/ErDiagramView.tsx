import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ReactFlow,
  Controls,
  MiniMap,
  Background,
  useNodesState,
  useEdgesState,
  ReactFlowProvider,
  ControlButton,
  type Node,
  type Edge,
  type NodeTypes,
  type Viewport,
  useReactFlow,
  BackgroundVariant,
  Panel,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { toPng, toSvg } from 'html-to-image';
import { Download, Search } from 'lucide-react';
import { databaseCommands } from '../../commands/database';
import { fileCommands } from '../../commands/file';
import { Button } from '../../components/ui/Button';
import { CopyableError } from '../../components/ui/CopyableError';
import { useI18n } from '../../hooks/useI18n';
import { buildErNodeContextMenuItems } from '../../lib/erNodeContextMenu';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { TableNode } from './er/TableNode';
import {
  buildErGraph,
  defaultCollapsedTables,
  dedupeSymmetricPredictions,
} from './er/buildErGraph';
import { ErRelationLegend } from './er/ErRelationLegend';
import {
  applyHoverToEdges,
  applyHoverToNodes,
  applyPinnedPositions,
  hoveredNeighbourhood,
} from './er/interactionState';
import type { ErPredictedRelation } from './er/buildErGraph';
import { toPredictionTablesFromSchemas } from '../../lib/relationPrediction/fromTableSchema';
import { predictRelations } from '../../lib/relationPrediction/predictRelations';
import { useSettingsStore } from '../../stores/settingsStore';
import type { TableSchema } from '../../types';
import { Spinner } from '../../components/ui/Spinner';

interface ErDiagramViewProps {
  dbSessionId: string;
  database: string;
  /**
   * Schema to read the diagram's tables from. Captured from the panel that was
   * active when the diagram was opened; `null` requests all catalog schemas.
   */
  schema?: string | null;
  focusTable?: string;
  onSelectTable?: (tableName: string, schema: string | null, database: string) => void;
  /** Optional; when omitted, Focus still works via internal focus state. */
  onFocusTable?: (tableName: string) => void;
}

const nodeTypes: NodeTypes = {
  tableNode: TableNode,
};

export function ErDiagramView(props: ErDiagramViewProps) {
  return (
    <ReactFlowProvider>
      <ErDiagramInner {...props} />
    </ReactFlowProvider>
  );
}

function ErDiagramInner({
  dbSessionId,
  database,
  schema = null,
  focusTable,
  onSelectTable,
  onFocusTable,
}: ErDiagramViewProps) {
  const { t } = useI18n();
  const { fitView, zoomIn, zoomOut } = useReactFlow();
  const [schemas, setSchemas] = useState<TableSchema[]>([]);
  // Inference is opt-in: an unset value means off, and the legend's button turns
  // it on for this and every other surface that reads the same setting.
  const fkPredictionEnabled = useSettingsStore((s) => s.settings.enableFkPrediction ?? false);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  /** Local focus override; syncs from prop when parent changes focusTable. */
  const [activeFocus, setActiveFocus] = useState<string | undefined>(focusTable);
  /**
   * Collapsed tables are view state, not node state.
   *
   * Collapsing changes a node's height, so it must re-run the layout — otherwise
   * the diagram keeps a collapsed node's old footprint and an expanded one can be
   * drawn over its neighbour. Holding it here (rather than in node data) is what
   * makes the rebuild happen at all.
   */
  const [collapsedTables, setCollapsedTables] = useState<ReadonlySet<string>>(
    () => new Set<string>(),
  );
  /**
   * Positions the user has dragged a node to.
   *
   * A relayout runs on schema load, on collapse and on focus, and it would
   * otherwise throw away every hand-placed node. Pinned nodes keep their position
   * and the "re-layout" control clears them.
   */
  const [pinnedPositions, setPinnedPositions] = useState<
    ReadonlyMap<string, { x: number; y: number }>
  >(() => new Map());
  /** Table under the cursor, whose relationships are brought forward. */
  const [hoveredTable, setHoveredTable] = useState<string | null>(null);
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // Controlled viewport — round x/y to eliminate sub-pixel blur (ReactFlow #3282)
  const [viewport, setViewport] = useState<Viewport>({ x: 0, y: 0, zoom: 1 });

  const onViewportChange = useCallback((vp: Viewport) => {
    setViewport({
      x: Math.round(vp.x),
      y: Math.round(vp.y),
      zoom: vp.zoom,
    });
  }, []);

  const onMoveEnd = useCallback((_: unknown, vp: Viewport) => {
    setViewport({
      x: Math.round(vp.x),
      y: Math.round(vp.y),
      zoom: vp.zoom,
    });
  }, []);

  useEffect(() => {
    setActiveFocus(focusTable);
  }, [focusTable]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    databaseCommands
      .getErData(dbSessionId, database, schema)
      .then((data) => {
        if (!cancelled) {
          setSchemas(data);
          // Wide tables start collapsed, but only as a seed: after this the user
          // owns the collapse state, so expanding one sticks.
          setCollapsedTables(defaultCollapsedTables(data));
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(String(e));
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [dbSessionId, database, schema]);

  // Inferred relationships, drawn alongside the declared ones. The ER diagram
  // already holds a full schema per table, so this needs no extra IPC — and it is
  // memoised on `schemas` because the engine walks every table in the database.
  const predictedRelations = useMemo<ErPredictedRelation[]>(() => {
    if (!fkPredictionEnabled || schemas.length < 2) return [];
    return predictRelations(toPredictionTablesFromSchemas(schemas)).map((candidate) => ({
      id: candidate.id,
      fromTable: candidate.fromTable,
      toTable: candidate.toTable,
      columnPairs: candidate.columnPairs,
      score: candidate.score,
      ambiguous: candidate.ambiguous,
    }));
  }, [fkPredictionEnabled, schemas]);

  /**
   * Search dims rather than filters — a hard filter would hide the very
   * neighbours that make a match understandable.
   */
  const applySearchState = useCallback(
    (list: Node[]): Node[] => {
      const query = searchQuery.trim().toLowerCase();
      return list.map((node) => {
        const name = (node.data.tableName as string).toLowerCase();
        const matches = query.length > 0 && name.includes(query);
        return {
          ...node,
          data: {
            ...node.data,
            highlighted: query.length > 0 ? matches : node.id === activeFocus,
            dimmed: query.length > 0 ? !matches : false,
          },
        };
      });
    },
    [searchQuery, activeFocus],
  );

  useEffect(() => {
    if (loading || error) return;
    const { nodes: n, edges: e } = buildErGraph(
      schemas,
      activeFocus,
      predictedRelations,
      collapsedTables,
    );
    // Search state is reapplied here because a relayout replaces every node.
    setNodes(applySearchState(applyPinnedPositions(n, pinnedPositions)));
    setEdges(e);
  }, [
    schemas,
    activeFocus,
    predictedRelations,
    collapsedTables,
    pinnedPositions,
    loading,
    error,
    setNodes,
    setEdges,
    applySearchState,
  ]);

  useEffect(() => {
    setNodes((nds) => applySearchState(nds));
  }, [applySearchState, setNodes]);

  useEffect(() => {
    const handler = (e: Event) => {
      const tableName = (e as CustomEvent<string>).detail;
      setCollapsedTables((current) => {
        const next = new Set(current);
        if (next.has(tableName)) next.delete(tableName);
        else next.add(tableName);
        return next;
      });
    };
    window.addEventListener('er-toggle-collapse', handler);
    return () => window.removeEventListener('er-toggle-collapse', handler);
  }, []);

  const stats = useMemo(() => {
    const tableCount = schemas.length;
    const declaredCount = schemas.reduce((acc, s) => acc + s.foreignKeys.length, 0);
    // Count what the diagram draws: the engine can guess the same link from both
    // sides, and a total that promises more lines than appear is its own confusion.
    const predictedCount = dedupeSymmetricPredictions(predictedRelations).length;
    return {
      tableCount,
      declaredCount,
      predictedCount,
      relationCount: declaredCount + predictedCount,
    };
  }, [schemas, predictedRelations]);

  const handleNodeDragStop = useCallback((_: unknown, node: Node) => {
    setPinnedPositions((current) => new Map(current).set(node.id, { ...node.position }));
  }, []);

  const handleRelayout = useCallback(() => {
    // Dropping the pins is the whole operation: the next build lays the graph out
    // again from the relationships.
    setPinnedPositions(new Map());
  }, []);

  const handleTogglePrediction = useCallback(
    (enabled: boolean) => {
      // Persisted, not page-local: the Settings → Editor switch shows the same
      // value, so the two can never disagree about whether inference is on.
      updateSettings({ enableFkPrediction: enabled }).catch((e: unknown) => {
        console.error('Failed to update smart foreign key prediction:', e);
      });
    },
    [updateSettings],
  );

  const handleNodeMouseEnter = useCallback((_: unknown, node: Node) => {
    setHoveredTable(node.id);
  }, []);

  const handleNodeMouseLeave = useCallback(() => {
    setHoveredTable(null);
  }, []);

  // Hover rather than click: clicking a node opens that table in the workspace and
  // leaves the diagram, so a click-driven highlight would never be seen.
  const neighbourhood = useMemo(
    () => hoveredNeighbourhood(edges, hoveredTable),
    [edges, hoveredTable],
  );
  const styledNodes = useMemo(
    () => applyHoverToNodes(nodes, neighbourhood),
    [nodes, neighbourhood],
  );
  const styledEdges = useMemo(() => applyHoverToEdges(edges, hoveredTable), [edges, hoveredTable]);

  const handleNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      if (onSelectTable && node.data?.tableName) {
        onSelectTable(node.data.tableName as string, schema, database);
      }
    },
    [onSelectTable, schema, database],
  );

  const handleFocusTable = useCallback(
    (tableName: string) => {
      setActiveFocus(tableName);
      onFocusTable?.(tableName);
    },
    [onFocusTable],
  );

  const handleNodeContextMenu = useCallback(
    (event: React.MouseEvent, node: Node) => {
      event.preventDefault();
      event.stopPropagation();
      const tableName = (node.data?.tableName as string | undefined) ?? node.id;
      if (!tableName) return;

      void showNativeContextMenu(
        buildErNodeContextMenuItems({
          labels: {
            openTable: t('schemaTree.openTable'),
            copyName: t('common.copyName'),
            focusTable: t('erDiagram.focusTable'),
          },
          handlers: {
            onOpenTable: onSelectTable
              ? () => {
                  onSelectTable(tableName, schema, database);
                }
              : undefined,
            onCopyName: () => {
              void navigator.clipboard.writeText(tableName);
            },
            onFocusTable: () => {
              handleFocusTable(tableName);
            },
          },
        }),
        { x: event.clientX, y: event.clientY },
      );
    },
    [t, onSelectTable, handleFocusTable, schema, database],
  );

  const handleExportPng = useCallback(async () => {
    const viewport = document.querySelector('.react-flow__viewport') as HTMLElement;
    if (!viewport) return;
    try {
      const dataUrl = await toPng(viewport, {
        backgroundColor: '#1a1a2e',
        quality: 1,
      });
      const base64 = dataUrl.includes(',') ? dataUrl.split(',')[1]! : dataUrl;
      await fileCommands.saveBase64WithDialog(base64, `er-diagram-${database}.png`, 'PNG', ['png']);
    } catch (e) {
      console.error('Export failed:', e);
    }
  }, [database]);

  const handleExportSvg = useCallback(async () => {
    const viewport = document.querySelector('.react-flow__viewport') as HTMLElement;
    if (!viewport) return;
    try {
      const dataUrl = await toSvg(viewport, {
        backgroundColor: '#1a1a2e',
      });
      let svgContent: string;
      if (dataUrl.startsWith('data:image/svg+xml;base64,')) {
        svgContent = atob(dataUrl.slice('data:image/svg+xml;base64,'.length));
      } else if (dataUrl.startsWith('data:image/svg+xml,')) {
        svgContent = decodeURIComponent(dataUrl.slice(dataUrl.indexOf(',') + 1));
      } else {
        svgContent = dataUrl;
      }
      await fileCommands.saveTextWithDialog(svgContent, `er-diagram-${database}.svg`, 'SVG', [
        'svg',
      ]);
    } catch (e) {
      console.error('Export failed:', e);
    }
  }, [database]);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner size="2xl" tone="muted" />
        <span className="ml-2 text-sm text-fg-muted">{t('erDiagram.loading')}</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full items-center justify-center p-4">
        <CopyableError message={error} className="max-w-lg text-sm text-red-400" />
      </div>
    );
  }

  if (schemas.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-fg-muted">
        {t('erDiagram.noTables')}
      </div>
    );
  }

  return (
    <div className="h-full w-full" data-testid="er-diagram-view">
      <ReactFlow
        nodes={styledNodes}
        edges={styledEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        onNodeDragStop={handleNodeDragStop}
        onNodeMouseEnter={handleNodeMouseEnter}
        onNodeMouseLeave={handleNodeMouseLeave}
        onNodeContextMenu={handleNodeContextMenu}
        nodeTypes={nodeTypes}
        viewport={viewport}
        onViewportChange={onViewportChange}
        onMoveEnd={onMoveEnd}
        fitView
        minZoom={0.1}
        maxZoom={10}
        proOptions={{ hideAttribution: true }}
        data-testid="er-diagram-flow"
        className="er-diagram-flow bg-surface"
      >
        <Controls
          data-testid="er-diagram-controls"
          showZoom={false}
          showFitView={false}
          showInteractive={false}
          className="!bg-surface !border-edge !shadow-lg [&>button]:!bg-surface [&>button]:!border-edge [&>button]:!text-fg-muted [&>button:hover]:!bg-surface-alt [&>button:hover]:!text-fg"
        >
          <ControlButton
            data-testid="er-diagram-zoom-in"
            aria-label={t('erDiagram.zoomIn')}
            title={t('erDiagram.zoomIn')}
            onClick={() => void zoomIn()}
          >
            +
          </ControlButton>
          <ControlButton
            data-testid="er-diagram-zoom-out"
            aria-label={t('erDiagram.zoomOut')}
            title={t('erDiagram.zoomOut')}
            onClick={() => void zoomOut()}
          >
            −
          </ControlButton>
          <ControlButton
            data-testid="er-diagram-relayout"
            aria-label={t('erDiagram.relayout')}
            title={t('erDiagram.relayout')}
            onClick={handleRelayout}
          >
            ⟲
          </ControlButton>
          <ControlButton
            data-testid="er-diagram-fit-view"
            aria-label={t('erDiagram.fitView')}
            title={t('erDiagram.fitView')}
            onClick={() => void fitView()}
          >
            ↗
          </ControlButton>
        </Controls>
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} className="!bg-surface" />
        <MiniMap
          pannable
          zoomable
          nodeColor={(node) => {
            if (node.data?.highlighted) return 'var(--color-accent, #2563eb)';
            return 'var(--color-surface-alt, #111827)';
          }}
          maskColor="rgba(0,0,0,0.5)"
          className="!bg-surface-alt !border-edge !shadow-lg"
        />
        <Panel
          position="top-left"
          className="flex items-center gap-2 rounded-lg bg-surface/80 px-2 py-1 backdrop-blur"
        >
          <Search className="h-3.5 w-3.5 text-fg-muted" />
          <input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t('common.search')}
            data-testid="er-diagram-search"
            className="h-7 w-40 bg-transparent text-xs text-fg outline-none placeholder:text-fg-muted"
          />
        </Panel>
        <Panel
          position="top-right"
          className="flex items-center gap-2 rounded-lg bg-surface/80 px-3 py-1.5 text-xs text-fg-muted backdrop-blur"
        >
          <div data-testid="er-diagram-stats" className="flex items-center gap-2">
            <Button
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              onClick={handleExportPng}
              title={t('erDiagram.exportPng')}
              data-testid="er-diagram-export-png"
            >
              <Download className="h-3.5 w-3.5" />
              PNG
            </Button>
            <Button
              variant="ghost"
              className="h-7 gap-1 px-2 text-xs"
              onClick={handleExportSvg}
              title={t('erDiagram.exportSvg')}
              data-testid="er-diagram-export-svg"
            >
              <Download className="h-3.5 w-3.5" />
              SVG
            </Button>
            <span className="text-edge">·</span>
            <span>{t('erDiagram.tableCount').replace('{count}', String(stats.tableCount))}</span>
            <span className="text-edge">·</span>
            <span>
              {t('erDiagram.relationCount').replace('{count}', String(stats.relationCount))}
            </span>
          </div>
        </Panel>
        {/* Bottom-centre: the corners are already taken by the search box, the
            stats, React Flow's zoom controls and the mini-map, and a legend that
            sat under one of them would be unreadable. */}
        <Panel position="bottom-center">
          {/* The key to the canvas's own lines, next to the button that adds the
              inferred ones — so "what is this amber dashed line" is answered
              where the question is asked. */}
          <ErRelationLegend
            declaredCount={stats.declaredCount}
            predictedCount={stats.predictedCount}
            predictionEnabled={fkPredictionEnabled}
            onTogglePrediction={handleTogglePrediction}
          />
        </Panel>
      </ReactFlow>
    </div>
  );
}
