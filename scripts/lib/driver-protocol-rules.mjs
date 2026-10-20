/**
 * The governed-contract rule table, stated as data.
 *
 * Moved out of `check-driver-protocol-compat.mjs` only to keep that file under
 * the size limit in AGENTS.md; the gate re-exports it, so the import path
 * every gate and test already uses is unchanged.
 *
 * @module lib/driver-protocol-rules
 */

/**
 * Public contracts of `packages/driver-api` whose shape is part of the
 * driver/host ABI.
 *
 * `kind` selects how a change inside the item's span is read:
 *
 * - `trait` — an added line that declares a method with no default body (ends
 *   in `;`) breaks every existing implementor, so it is breaking. An added
 *   line with a default body (`{`) is additive.
 * - `struct` — an added line is additive, provided the fail-closed `Default`
 *   invariant holds on the Rust side. A removed line is breaking.
 * - `enum` — the *members* decide, not the lines. A removed or renamed variant
 *   is breaking, and so is an added variant unless the enum carried
 *   `#[non_exhaustive]` at the base ref. See `lib/driver-protocol-members.mjs`
 *   for why an addition is not automatically additive: without
 *   `#[non_exhaustive]` a downstream `match` must be exhaustive, so a new
 *   variant is rustc E0004 in every out-of-tree driver that already compiled.
 *
 * `anchor` must match the item's declaration line and must be unambiguous
 * within the file; it is matched by `includes`. `scripts/__tests__/check-driver-protocol-compat.test.ts`
 * asserts both properties against the real source tree.
 *
 * `previousFile` names where a contract lived before it was moved to its own
 * module. It is read only when `file` has no blob at the base ref, so it is the
 * base-side half of a relocation rather than a second source of truth, and it
 * self-invalidates: once the base ref has moved past the move, the anchor is no
 * longer in `previousFile` and the rule reads as a brand-new addition, exactly
 * as it would have without the field.
 *
 * @type {ReadonlyArray<{
 *   id: string,
 *   file: string,
 *   anchor: string,
 *   kind: 'trait' | 'struct' | 'enum',
 *   previousFile?: string,
 *   implementedBy?: string,
 *   why?: string,
 * }>}
 */
export const CONTRACT_RULES = Object.freeze([
  {
    id: 'database-driver',
    file: 'packages/driver-api/src/traits.rs',
    anchor: 'pub trait DatabaseDriver',
    kind: 'trait',
    implementedBy: 'every out-of-tree driver',
    why: 'The trait every driver implements; a signature change is a recompile requirement for the whole fleet.',
  },
  {
    id: 'sql-dialect',
    file: 'packages/driver-api/src/dialect.rs',
    anchor: 'pub trait SqlDialect',
    kind: 'trait',
    implementedBy: 'the host SQL builders and, once migrated, every SQL driver',
    why: 'The narrow text contract a SQL builder consumes instead of the whole driver. Its methods are the quoting, pagination, literal and placeholder rules; a signature change recompiles every consumer that was moved off `DatabaseDriver`, including the shared builders in this crate.',
  },
  {
    id: 'legacy-sql-dialect',
    // Anchored on the inherent impl, not on `pub struct LegacySqlDialect`:
    // the struct body is one private borrow field, and governing it would turn
    // an internal field rename into a contract event. The host constructs this
    // adapter to keep driving un-migrated consumers, so its constructor is the
    // real binding point.
    file: 'packages/driver-api/src/dialect.rs',
    anchor: "impl<'a, D: DatabaseDriver + ?Sized> LegacySqlDialect<'a, D> {",
    kind: 'struct',
    implementedBy: 'the host, wrapping drivers whose call sites have not moved yet',
    why: 'The one-way adapter from `DatabaseDriver` to `SqlDialect`. Its constructor signature is called at every migrated consumer, so it is the migration seam between the old and new contracts and cannot change silently.',
  },
  {
    id: 'catalog-reader',
    file: 'packages/driver-api/src/catalog.rs',
    anchor: 'pub trait CatalogReader',
    kind: 'trait',
    implementedBy: 'the host schema cache and metadata adapters; every SQL driver once migrated',
    why: 'The explicit-target catalog surface (databases, schemas, relations, columns). The schema cache is built on it, so a signature change reaches every cache consumer without touching `DatabaseDriver`.',
  },
  {
    id: 'legacy-catalog-reader',
    // The inherent impl is the binding point: the host constructs this adapter
    // wherever a `CatalogReader` is required but the caller still holds a
    // driver. Structure follows `legacy-sql-dialect` for the same reason.
    file: 'packages/driver-api/src/catalog.rs',
    anchor: "impl<'a, D: DatabaseDriver + ?Sized> LegacyCatalogReader<'a, D> {",
    kind: 'struct',
    implementedBy: 'the host, wrapping drivers at un-migrated call sites',
    why: 'The one-way adapter from `DatabaseDriver` to `CatalogReader`. Its constructor signature is called at every migrated catalog consumer, so it is the migration seam and cannot change silently.',
  },
  {
    id: 'bound-parameter-limit',
    file: 'packages/driver-api/src/dialect.rs',
    anchor: 'pub enum BoundParameterLimit',
    kind: 'enum',
    implementedBy: 'every SqlDialect implementation, read by batch writers',
    why: 'The provenance of a statement\'s bound-parameter ceiling. Batch splitting reads the value, and documentation reads whether it can be tuned; adding or renaming a variant changes what a driver is allowed to claim, and an exhaustive match in the host stops compiling without `#[non_exhaustive]`.',
  },
  {
    id: 'key-value-driver',
    // Declared in its own module since `traits.rs` was split; `traits.rs` still
    // re-exports it, so the public path never changed.
    file: 'packages/driver-api/src/traits/key_value.rs',
    previousFile: 'packages/driver-api/src/traits.rs',
    anchor: 'pub trait KeyValueDriver',
    kind: 'trait',
    implementedBy: 'key/value drivers',
    why: 'Same reasoning as DatabaseDriver, on the narrower KV surface.',
  },
  {
    id: 'driver-factory',
    file: 'packages/driver-api/src/factory.rs',
    anchor: 'pub trait DatabaseDriverFactory',
    kind: 'trait',
    implementedBy: 'every out-of-tree driver',
    why: 'Host registration goes through factories only (src-tauri DriverRegistry); changing it changes the discovery contract.',
  },
  {
    id: 'resource-provider',
    file: 'packages/driver-api/src/resource.rs',
    anchor: 'pub trait ResourceProvider',
    kind: 'trait',
    implementedBy: 'every out-of-tree driver',
    why: 'Supplies handles, namespaces and command execution to the host.',
  },
  {
    id: 'budget-port',
    file: 'packages/driver-api/src/resource.rs',
    anchor: 'pub trait BudgetPort',
    kind: 'trait',
    implementedBy: 'the host, consumed by drivers',
    why: 'The downlink channel for physical quota; the host implements it and drivers release against it.',
  },
  {
    id: 'resource-error',
    file: 'packages/driver-api/src/resource.rs',
    anchor: 'pub enum ResourceError',
    kind: 'enum',
    implementedBy: 'both sides, as an error contract',
    why: 'Error variants are matched on by the host and by every driver that renders a failure back to the caller. It carries no #[non_exhaustive], so the match is exhaustive on both sides and *any* change to the variant list — renaming or removing one, and adding one just as much — stops an out-of-tree `match` from compiling.',
  },
  {
    id: 'driver-command-definition',
    file: 'packages/driver-api/src/command.rs',
    anchor: 'pub struct DriverCommandDefinition',
    kind: 'struct',
    implementedBy: 'drivers produce it, host dispatches it',
    why: 'Every SQL-editor action routes through execute_driver_command, so this struct is the command wire shape.',
  },
  {
    id: 'capability-set',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub struct CapabilitySet',
    kind: 'struct',
    implementedBy: 'every out-of-tree driver',
    why: 'The declaration a driver hands the host; a new domain is additive exactly because its Default is Unknown.',
  },
  {
    id: 'capability-registry',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub struct CapabilityRegistry',
    kind: 'struct',
    implementedBy: 'the host',
    why: 'The fail-closed surface that turns an Unsupported capability into an error instead of a silent no-op.',
  },
  {
    id: 'reuse-driver',
    file: 'packages/driver-api/src/reuse.rs',
    anchor: 'pub struct ReuseDriver',
    kind: 'struct',
    implementedBy: 'every out-of-tree driver',
    why: 'Constructed by every driver to satisfy the host; field visibility changes alter what a driver may even write.',
  },
  // The capability enums below are the values a driver *writes into*
  // CapabilitySet and the host *matches on* cell by cell. They are not
  // internal detail: each one is an exhaustively matched, non-#[non_exhaustive]
  // enum in both directions. `#[non_exhaustive]` appears nowhere under
  // packages/driver-api/src, so the added-variant branch of the classifier
  // resolves to `breaking` for every one of them — which is the point, and the
  // reason `NamespaceSwitch::PerRequest` had to be added by hand before it was
  // noticed here.
  {
    id: 'availability',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum Availability',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, as CapabilitySet::stateful_session',
    why: 'The coarse supported/unsupported/unknown answer the host gates every stateful-session feature on. A driver writes the variant and the host matches it; with no #[non_exhaustive] both sides must stay exhaustive, so adding a variant is E0004 in the driver fleet and renaming one is the same.',
  },
  {
    id: 'namespace-switch',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum NamespaceSwitch',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, as CapabilitySet::namespace_switch',
    why: 'How a driver reaches a namespace other than the connected one. It is the enum that already proved the point: PerRequest was added as a variant and nothing outside driver-api could see the change. Adding PerRequest meant every driver that matched InPlace | RequiresReplacement | Unsupported | Unknown had to be edited and recompiled.',
  },
  {
    id: 'context-observation',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum ContextObservation',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, as CapabilitySet::context_observation',
    why: 'How completely a driver can report observable session context. Only Full yields a confirmed snapshot, so the host matches on it per capability cell; an added variant is an unhandled arm in every driver that builds against the old crate.',
  },
  {
    id: 'transaction-observation',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum TransactionObservation',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, as CapabilitySet::transaction_observation',
    why: 'How completely a driver can report transaction state. The host matches transaction state per session, so a new variant stops an exhaustive match in a driver compiling and a rename stops the host matching the state at all.',
  },
  {
    id: 'session-continuity',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum SessionContinuity',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, via CapabilitySet::stateful_session',
    why: 'The Fixed/Leased split the migration doc makes load-bearing: Fixed may only be declared by a driver that really owns a session-scoped physical connection, and a pool of size one is not evidence. Widening or narrowing that set is a change to what a driver is allowed to claim, so the variant list itself is the contract.',
  },
  {
    id: 'session-scoped-handles',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum SessionScopedHandleSupport',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, as CapabilitySet::session_scoped_handles',
    why: 'Whether a driver may hand out cursors, prepared statements and transactions that outlive a call. The host matches it before trusting a handle list, so a new variant is an unhandled arm on both sides of the boundary.',
  },
  {
    id: 'reset-for-reuse',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum ResetForReuse',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, as CapabilitySet::reset_for_reuse',
    why: 'Whether resetResource has been *verified* to return a resource to its initialization baseline — a claim the host acts on by reusing the resource, so the variant list is part of what a driver may assert.',
  },
  {
    id: 'precise-cancel',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum PreciseCancelSupport',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, as CapabilitySet::precise_cancel',
    why: 'Whether requestCancel may address a single execution. Unknown is explicitly not "cancel succeeded", so the host matches this cell before forwarding anything; an added variant has to be handled everywhere, not ignored.',
  },
  {
    id: 'snapshots',
    file: 'packages/driver-api/src/capabilities.rs',
    anchor: 'pub enum SnapshotSupport',
    kind: 'enum',
    implementedBy: 'every out-of-tree driver, as CapabilitySet::snapshots',
    why: 'The named consistency guarantees for read snapshots (per table / per database / coordinated). Every value other than Unsupported is a real guarantee the host relies on, so the variant list is a promise the driver makes and cannot silently widen.',
  },
  {
    id: 'legacy-resource-adapter',
    // Anchored on the inherent impl, not on `pub struct LegacyResourceAdapter`.
    //
    // That placement is the decision, and it is deliberate. Two spans were
    // candidates and this comment exists so the choice does not read as
    // accidental or as "either works":
    //
    // - `pub struct LegacyResourceAdapter` resolves to lines 76-83: six
    //   *private* fields. Dropping or renaming one is internal bookkeeping
    //   inside driver-api and recompiles nothing outside the crate, so
    //   governing it would manufacture "breaking" out of edits that cannot
    //   break anyone.
    // - `impl LegacyResourceAdapter` resolves to lines 111-287, which is where
    //   the host's actual binding point lives: `new(driver, driver_id,
    //   driver_version)` and `with_evidence`. The host calls these to wrap
    //   every un-migrated path driver, so changing either signature is a
    //   recompile requirement for the host and the whole un-migrated fleet —
    //   the exact thing this gate exists to make visible before it is merged
    //   rather than at the next protocol bump.
    //
    // The `ResourceProvider` surface the adapter implements is already
    // governed by the `resource-provider` rule above; what was ungoverned was
    // the adapter's own inherent surface. That gap is what this rule closes.
    file: 'packages/driver-api/src/resource_adapter.rs',
    anchor: 'impl LegacyResourceAdapter {',
    kind: 'struct',
    implementedBy: 'the host, wrapping every un-migrated path driver',
    why: 'The shim that lets a driver that has not implemented ResourceProvider be driven through it anyway. Its constructor signature is called by the host once per un-migrated driver, so it is a driver/host ABI surface even though it lives in a helper type rather than in a trait.',
  },
]);
