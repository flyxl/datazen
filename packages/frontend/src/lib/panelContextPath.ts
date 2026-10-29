/** Breadcrumb for table/view panel sub-tab bars: `connection.database.schema`. */
export function formatPanelContextPath(args: {
  connectionName: string;
  database?: string | null;
  schema?: string | null;
}): string {
  const parts = [args.connectionName];
  const database = args.database?.trim();
  if (database) parts.push(database);
  const schema = args.schema?.trim();
  if (schema) parts.push(schema);
  return parts.join('.');
}
