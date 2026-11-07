/**
 * @datazen/frontend/app — the app bootstrap entry (host-boundary follow-up F2).
 *
 * Separate from the light `.` entry on purpose: `startSharedApp` transitively
 * owns the whole shared app graph (App, stores, PRO host-module table, global
 * CSS), and the desktop platform adapters consume only the light surface
 * (ports, contracts, capability bits). Keeping the bootstrap out of `.` means
 * a light consumer's test graph — and its partial `vi.mock`s of frontend
 * internals — never has to know the app graph exists.
 */
export { startSharedApp, type StartSharedAppOptions } from './bootstrap/startSharedApp';
