import { useCallback, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  AlertTriangle,
  BarChart3,
  History,
  LineChart as LineChartIcon,
  Loader2,
  Maximize2,
  Minimize2,
  Pencil,
  PieChart as PieChartIcon,
  RefreshCw,
  ScatterChart as ScatterChartIcon,
  Trash2,
  TrendingUp,
} from 'lucide-react';
import { ChartCanvas } from '../../components/chart/ChartCanvas';
import { AxisConfigurator } from '../../components/chart/AxisConfigurator';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { Button } from '../../components/ui/Button';
import { hasRenderableChart, widgetRunToChartData } from '../../lib/dashboard/runToChart';
import { widgetRunToStatementResult } from '../../lib/dashboard/runToResult';
import { inferAllFields } from '../../lib/chart/fieldInference';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import type { DashboardWidget, ViewMode, WidgetRun } from '../../types/dashboard';
import type { ChartConfig, ChartType } from '../../types/chart';
