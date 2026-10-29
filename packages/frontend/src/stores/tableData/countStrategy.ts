import { useSchemaStore } from '../schemaStore';
import { useSettingsStore } from '../settingsStore';
import { resolveCountStrategy } from '../../lib/rowCountStrategy';
import type { TableChangeContext } from '../../lib/tableChanges';
import type { CountStrategy } from '../../types';
import {
  DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD,
  DEFAULT_ROW_COUNT_MODE,
} from '../../types/settings';

/**
 * The row count the catalog already knows for this table, if any.
 *
 * It is a *prior*, not a fact: the `auto` mode weighs it against the
 * threshold, and the driver's own answer still wins when the prior is
 * missing.
 */
function catalogRowCount(context: TableChangeContext): number | undefined {
  const tables = useSchemaStore.getState().getConnectionSchema(context.dbSessionId)?.tables;
  if (!tables) return undefined;
  const match = tables.find(
    (info) =>
      info.name === context.table &&
      (!context.schema || !info.schema || info.schema === context.schema),
  );
  return match?.rowCount ?? undefined;
}

/**
 * The count strategy a read of this panel asks for.
 *
 * **One** implementation, and every read path goes through it: the panel's
 * first read *and* every re-read after it — a page move, a filter change, a
 * manual reload.
 *
 * That matters because the `auto` mode weighs the size the catalog knows. If
 * only the first read weighed it, a table **below** the threshold would ask
 * for an exact count at first and for nothing at all afterwards — and
 * "nothing" means the driver's own declaration, which for an estimating
 * driver is an estimate. The panel's number would then change shape on the
 * first refresh (`1-50 / 100` becomes `1-50 / ≈ 100`).
 *
 * The mapping itself (`rowCountMode → countStrategy`) stays in
 * `lib/rowCountStrategy`: the contract decides it, this function only feeds
 * it the prior it needs.
 */
export function resolvePanelCountStrategy(
  context: TableChangeContext,
): CountStrategy | undefined {
  const settings = useSettingsStore.getState().settings;
  return resolveCountStrategy({
    mode: settings.rowCountMode ?? DEFAULT_ROW_COUNT_MODE,
    threshold: settings.rowCountEstimateThreshold ?? DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD,
    knownRowCount: catalogRowCount(context),
  });
}
