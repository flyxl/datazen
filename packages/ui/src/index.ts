export { cn } from './cn';
export { tid, type TidAttrs } from './tid';
export { Button, type ButtonProps } from './Button';
export { Input, type InputProps } from './Input';
// The one themed checkbox / radio. Replaces the hand-written
// `accent-accent`-tinted native controls; atomic boxes, callers keep their
// `<label>` wrapper (click target + a11y association), so `data-testid`
// placement is unchanged.
export { Checkbox, type CheckboxProps } from './Checkbox';
export { Radio, type RadioProps } from './Radio';
export {
  Select,
  defaultSelectLabels,
  type SelectOption,
  type SelectProps,
  type SelectLabels,
} from './Select';
// The `data-*` passthrough contract itself (see docs/architecture/frontend/components.md §9.1).
// Exported so the other closed-prop components can implement the *same* contract
// from outside this package rather than re-declaring it locally.
export { splitDataAttrs, type DataAttrProps, type SplitDataAttrs } from './dataAttrs';
export { Dialog, type DialogProps } from './Dialog';
export { Tabs, type TabItem, type TabRenderContext, type TabsProps } from './Tabs';
export { Badge, type BadgeProps } from './Badge';
export { Label, type LabelProps } from './Label';
// The one loading indicator. Replaces ~100 hand-written `animate-spin` call
// sites, and gives the ones that never spoke to a screen reader a way to.
export {
  Spinner,
  type SpinnerProps,
  type SpinnerSize,
  type SpinnerTone,
  type SpinnerVariant,
} from './Spinner';
export { Slider, type SliderProps } from './Slider';
export {
  TemporalValueInput,
  type TemporalValueInputProps,
  type TemporalPickerKind,
} from './TemporalValueInput';
export {
  PathInput,
  type PathInputProps,
  type PathPicker,
  type PathPickerDialogOptions,
} from './PathInput';
export { ToolbarShell, type ToolbarShellProps } from './ToolbarShell';
export { ToolbarButton, type ToolbarButtonProps } from './ToolbarButton';
export { ConfirmDialog, type ConfirmDialogProps } from './ConfirmDialog';
export { CopyableError, type CopyableErrorProps } from './CopyableError';
// The one error bar, so host pages, settings dialogs and driver UI stop
// hand-rolling `role="alert"` plus a private shade of red at every call site.
export {
  ErrorBanner,
  type ErrorBannerElement,
  type ErrorBannerProps,
  type ErrorBannerVariant,
} from './ErrorBanner';
export { ResultMessageDialog, type ResultMessageDialogProps } from './ResultMessageDialog';
export { LimitationsDialog, type LimitationsDialogProps } from './LimitationsDialog';
// Shared tree row contract + level/indent/navigation arithmetic. Pure: the
// host navigator and the driver key browsers both flatten their business
// objects into the same pre-order row list and used to answer these questions
// separately.
export { TREE_TOP_LEVEL, type TreeRow, type TreeRowLevel, type TreeRowNode } from './tree/types';
export {
  ariaLevelOf,
  indentOf,
  rowLevels,
  searchedRowLevels,
  type ResolvedRowLevel,
} from './tree/geometry';
export {
  ancestorIndexes,
  descendantIndexes,
  descendantRange,
  effectiveExpanded,
  firstChildIndex,
  nextNavigableIndex,
  parentIndexOf,
  showsSubtree,
  stepIndex,
  type BranchProbe,
} from './tree/navigation';
// The shared tree widget: one virtualization + ARIA + keyboard mechanism, so
// the host navigator and the driver key browsers stop hand-rolling four copies
// of it (and stop disagreeing about what a stable row key is).
export {
  VirtualTree,
  firstVisibleTreeIndex,
  type VirtualTreeItemAria,
  type VirtualTreeNavigation,
  type VirtualTreeOverlayContext,
  type VirtualTreeProps,
  type VirtualTreeRowContext,
} from './tree/VirtualTree';
export {
  planTreeNavigation,
  type TreeKeyEventLike,
  type TreeNavAction,
  type TreeNavCommand,
  type TreeNavMove,
  type TreeNavNone,
  type TreeNavPlan,
  type TreeNavPlanInput,
  type TreeNavRowAction,
} from './tree/keyboard';
// Shared "copy to clipboard" confirmation. Exported so the host, drivers and
// extensions converge on the same optimistic-rollback semantics instead of
// re-implementing a 1.5s `setCopied(true)` / `setTimeout` pair per call site.
export { useCopyFeedback } from './useCopyFeedback';
// The ONE i18n implementation shared by host, drivers and extensions.
export {
  setLocale,
  getLocale,
  registerTranslations,
  getRegisteredTranslations,
  t,
  useI18n,
  type I18nParams,
} from './i18n';
