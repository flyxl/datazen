import { useCallback, useEffect, useState } from 'react';
import {
  BookOpen,
  Download,
  Gauge,
  Loader2,
  Pause,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  Upload,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import { useLocaleDomains } from '../../hooks/useLocaleDomains';
import { LocaleDomainLoading } from '../../components/LocaleDomainLoading';
import { openDocsWindow, openWorkflowWindow } from '../../lib/windowManager';
import { dashboardCommands } from '../../commands/dashboard';
import { aiCommands } from '../../commands/ai';
import { useDashboardStore } from '../../stores/dashboardStore';
import { DEFAULT_CHART_CONFIG } from '../../types/chart';
import type { ChartConfig, ChartType } from '../../types/chart';
import type { Dashboard, DashboardWidget, ViewMode } from '../../types/dashboard';
import type { WorkflowListItem } from '../../types';
import { DEFAULT_REFRESH } from '../../types/dashboard';
import { ChartWidgetTile } from './ChartWidgetTile';
import { RunHistoryDrawer } from './RunHistoryDrawer';
import { WidgetEditorDrawer } from './WidgetEditorDrawer';

export function createEmptyDashboard(name: string): Dashboard {
  const now = new Date().toISOString();
  return {
    id: crypto.randomUUID(),
    name,
    createdAt: now,
    updatedAt: now,
    layout: { cols: 12, rowHeight: 80 },
    widgets: [],
    enabled: true,
  };
}

function nextWidgetLayout(widgets: DashboardWidget[]): DashboardWidget['layout'] {
  const maxY = widgets.reduce((m, w) => Math.max(m, w.layout.y + w.layout.h), 0);
  return { x: 0, y: maxY, w: 6, h: 4 };
}

export interface DashboardPanelProps {
  initialDashboardId?: string;
  onDashboardChange?: (id: string, name: string) => void;
  onOpenWorkflowEditor?: () => void;
}

export function DashboardPanel({
  initialDashboardId,
  onDashboardChange,
  onOpenWorkflowEditor,
}: DashboardPanelProps) {
  const localesReady = useLocaleDomains(['dashboard']);
  const { t } = useI18n();
  const [activeDashboardId, setActiveDashboardId] = useState(initialDashboardId ?? '');
  const dashboardId = activeDashboardId;

  const entry = useDashboardStore((s) => s.dashboardsById[dashboardId]);
  const current = entry?.dashboard ?? null;
  const runs = entry?.runs ?? {};
  const busyWidgets = entry?.busyWidgets ?? {};
  const loading = entry?.loading ?? false;
  const error = entry?.error ?? null;
  const list = useDashboardStore((s) => s.list);
  const listLoading = useDashboardStore((s) => s.listLoading);
  const mountDashboard = useDashboardStore((s) => s.mountDashboard);
  const loadDashboard = useDashboardStore((s) => s.loadDashboard);
  const saveDashboard = useDashboardStore((s) => s.saveDashboard);
  const fetchDashboards = useDashboardStore((s) => s.fetchDashboards);
  const deleteDashboard = useDashboardStore((s) => s.deleteDashboard);
  const refreshWidget = useDashboardStore((s) => s.refreshWidget;
