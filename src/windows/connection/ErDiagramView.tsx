import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ReactFlow,
  Controls,
  Background,
  useNodesState,
  useEdgesState,
  ReactFlowProvider,
  type Node,
  type Edge,
  type NodeTypes,
  BackgroundVariant,
  Panel,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { toPng, toSvg } from 'html-to-image';
import { Download, Loader2, Search } from 'lucide-react';
import { databaseCommands } from '../../commands/database';
import { fileCommands } from '../../commands/file';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { TableNode } from './er/TableNode';
import { buildErGraph } from './er/buildErGraph';
import type { TableSchema } from '../../types';

interface ErDiagramViewProps {
  connectionId: string;
  database: string;
  focusTable?: string;
  onSelectTable?: (tableName: string) => void;
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

function ErDiagramInner({ connectionId, database, focusTable, onSelectTable }: ErDiagramViewProps) {
  const { t } = useI18n();
  const [schemas, setSchemas] = useState<TableSchema[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    databaseCommands
      .getErData(connectionId, database)
      .then((data) => {
        if (!cancelled) {
          setSchemas(data);
          const { nodes: n, edges: e } = buildErGraph(data, focusTable);
          setNodes(n);
          setEdges(e);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(String(e));
          setLoading(false);
        }
      });
    return () => { cancelled = true; };
  }, [connectionId, database, focusTable, setNodes, setEdges]);

  useEffect(() => {
    if (!searchQuery.trim()) {
      setNodes((nds) =>
        nds.map((n) => ({
          ...n,
          data: {
            ...n.data,
            highlighted: n.id === focusTable,
            dimmed: false,
          },
        })),
      );
      return;
    }
    const q = searchQuery.toLowerCase();
    setNodes((nds) =>
      nds.map((n) => ({
        ...n,
        data: {
          ...n.data,
          highlighted: (n.data.tableName as string).toLowerCase().includes(q),
          dimmed: !(n.data.tableName as string).toLowerCase().includes(q),
        },
      })),
    );
  }, [searchQuery, setNodes, focusTable]);

  useEffect(() => {
    const handler = (e: Event) => {
      const tableName = (e as CustomEvent<string>).detail;
      setNodes((nds) =>
        nds.map((n) =>
          n.id === tableName
            ? { ...n, data: { ...n.data, collapsed: !n.data.collapsed } }
            : n,
        ),
      );
    };
    window.addEventListener('er-toggle-collapse', handler);
    return () => window.removeEventListener('er-toggle-collapse', handler);
  }, [setNodes]);

  const stats = useMemo(() => {
    const tableCount = schemas.length;
    const relationCount = schemas.reduce((acc, s) => acc + s.foreignKeys.length, 0);
    return { tableCount, relationCount };
  }, [schemas]);

  const handleNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      if (onSelectTable && node.data?.tableName) {
        onSelectTable(node.data.tableName as string);
      }
    },
    [onSelectTable],
  );

  const handleExportPng = useCallback(async () => {
    const viewport = document.querySelector('.react-flow__viewport') as HTMLElement;
    if (!viewport) return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const filePath = await save({
        defaultPath: `er-diagram-${database}.png`,
        filters: [{ name: 'PNG', extensions: ['png'] }],
      });
      if (!filePath) return;

      const dataUrl = await toPng(viewport, {
        backgroundColor: '#1a1a2e',
        quality: 1,
      });
      const base64 = dataUrl.includes(',') ? dataUrl.split(',')[1]! : dataUrl;
      await fileCommands.writeFileBase64(filePath, base64);
    } catch (e) {
      console.error('Export failed:', e);
    }
  }, [database]);

  const handleExportSvg = useCallback(async () => {
    const viewport = document.querySelector('.react-flow__viewport') as HTMLElement;
    if (!viewport) return;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const filePath = await save({
        defaultPath: `er-diagram-${database}.svg`,
        filters: [{ name: 'SVG', extensions: ['svg'] }],
      });
      if (!filePath) return;

      const dataUrl = await toSvg(viewport, {
        backgroundColor: '#1a1a2e',
      });
      // data:image/svg+xml;charset=utf-8,... or base64
      let svgContent: string;
      if (dataUrl.startsWith('data:image/svg+xml;base64,')) {
        svgContent = atob(dataUrl.slice('data:image/svg+xml;base64,'.length));
      } else if (dataUrl.startsWith('data:image/svg+xml,')) {
        svgContent = decodeURIComponent(dataUrl.slice(dataUrl.indexOf(',') + 1));
      } else {
        svgContent = dataUrl;
      }
      await fileCommands.writeFile(filePath, svgContent);
    } catch (e) {
      console.error('Export failed:', e);
    }
  }, [database]);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-6 w-6 animate-spin text-fg-muted" />
        <span className="ml-2 text-sm text-fg-muted">{t('erDiagram.loading')}</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-red-400">{error}</div>
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
    <div className="h-full w-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        nodeTypes={nodeTypes}
        fitView
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        className="bg-surface"
      >
        <Controls
          showInteractive={false}
          className="!bg-surface !border-edge !shadow-lg [&>button]:!bg-surface [&>button]:!border-edge [&>button]:!text-fg-muted [&>button:hover]:!bg-surface-alt [&>button:hover]:!text-fg"
        />
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} className="!bg-surface" />
        <Panel position="top-left" className="flex items-center gap-2 rounded-lg bg-surface/80 px-2 py-1 backdrop-blur">
          <Search className="h-3.5 w-3.5 text-fg-muted" />
          <input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t('common.search')}
            className="h-7 w-40 bg-transparent text-xs text-fg outline-none placeholder:text-fg-muted"
          />
        </Panel>
        <Panel position="top-right" className="flex items-center gap-2 rounded-lg bg-surface/80 px-3 py-1.5 text-xs text-fg-muted backdrop-blur">
          <Button variant="ghost" className="h-7 gap-1 px-2 text-xs" onClick={handleExportPng} title={t('erDiagram.exportPng')}>
            <Download className="h-3.5 w-3.5" />
            PNG
          </Button>
          <Button variant="ghost" className="h-7 gap-1 px-2 text-xs" onClick={handleExportSvg} title={t('erDiagram.exportSvg')}>
            <Download className="h-3.5 w-3.5" />
            SVG
          </Button>
          <span className="text-edge">·</span>
          <span>{t('erDiagram.tableCount').replace('{count}', String(stats.tableCount))}</span>
          <span className="text-edge">·</span>
          <span>{t('erDiagram.relationCount').replace('{count}', String(stats.relationCount))}</span>
        </Panel>
      </ReactFlow>
    </div>
  );
}
