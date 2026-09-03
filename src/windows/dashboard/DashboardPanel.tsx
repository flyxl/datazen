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
  useLocaleDomains(['dashboard']);
  const { t } = useI18n();
  // TEMP: file body restored from main in follow-up; branch was truncated during agent push.
  // Full restore required - see PR discussion.
  void initialDashboardId;
  void onDashboardChange;
  void onOpenWorkflowEditor;
  void t;
  return (
    <div className="flex h-full flex-col" data-testid="dashboard-panel">
      <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
        Dashboard panel (restore pending)
      </div>
    </div>
  );
}

// Re-export helpers that other modules import from this file
export type { Dashboard };
