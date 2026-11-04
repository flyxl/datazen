export interface SqlServerConnectionOptions {
  trustServerCertificate?: boolean;
  applicationName?: string;
}

export function readSqlServerOptions(
  raw: Record<string, unknown> | undefined,
): SqlServerConnectionOptions {
  if (!raw) return {};
  return {
    trustServerCertificate:
      typeof raw.trustServerCertificate === 'boolean' ? raw.trustServerCertificate : undefined,
    applicationName:
      typeof raw.applicationName === 'string' && raw.applicationName.trim()
        ? raw.applicationName.trim()
        : undefined,
  };
}

export function mergeSqlServerOptions(
  raw: Record<string, unknown>,
  patch: Partial<SqlServerConnectionOptions>,
): Record<string, unknown> {
  const next = { ...raw };
  if (patch.trustServerCertificate !== undefined) {
    next.trustServerCertificate = patch.trustServerCertificate;
  }
  if (patch.applicationName !== undefined) {
    const value = patch.applicationName.trim();
    if (value) next.applicationName = value;
    else delete next.applicationName;
  }
  return next;
}
