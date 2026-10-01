import type { DriverFormValidator } from '@datazen/driver-sdk';

/**
 * Raw connection-form fields handed to a driver validator. Aliased off the SDK
 * contract instead of re-declared (the redis validator re-spells the object
 * shape) so a field added upstream cannot silently drift away from the type the
 * host actually passes in `useConnectionForm.validate()`.
 */
export type SqlServerValidationFields = Parameters<DriverFormValidator>[0];

/** Translates an i18n key, matching the second parameter of `DriverFormValidator`. */
export type SqlServerValidationTranslate = Parameters<DriverFormValidator>[1];

/**
 * SQL Server is a `connectionMode: 'server'` driver, and `ConnectionFields.tsx`
 * renders both `host` and `port` as `<Label required>` with a dedicated
 * `form.validationErrors.*` slot each. Those two slots used to be unreachable:
 * the form registers a *driver form* (`isDriverForm === true`), so the generic
 * `!isDriverForm` branch in the host was skipped, and no driver validator was
 * registered either — `getDriverValidator('sqlserver')` returned `undefined`, so
 * the host's driver-validator early return never fired and nothing at all looked
 * at host/port. This function is that missing layer.
 *
 * Rules are kept byte-identical to the `standalone` branch of
 * `validateRedisConnection` (redis `connectionWizardValidate.ts`) so the two
 * driver validators stay behaviourally interchangeable for the host: same
 * "blank means blank after trim" reading of `host`, same
 * "blank or not-a-number means invalid" reading of `port`, same `t()` keys, same
 * empty-object-means-valid convention.
 *
 * Deliberately *not* validated here, matching the rendered form: `database`,
 * `username` and `password` carry no `<Label required>` and no error slot, and
 * SQL Server resolves those against server defaults (`master`, `sa`) when blank.
 * `databaseFieldType: 'name'` plus `hasMultiDatabase` means the host may pass an
 * empty database legitimately, so requiring it would mis-block a valid connection.
 */
export function validateSqlServerConnection(
  fields: SqlServerValidationFields,
  t: SqlServerValidationTranslate,
): Record<string, string> {
  const errors: Record<string, string> = {};
  if (!fields.host.trim()) errors.host = t('newConn.required');
  if (!fields.port.trim() || Number.isNaN(Number(fields.port))) {
    errors.port = t('newConn.required');
  }
  return errors;
}