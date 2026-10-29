/** Green success panel shown after a single-table export completes. */
export function ExportSuccessPanel({ message }: { message: string }) {
  return (
    <div className="flex flex-col items-center gap-2 py-6 text-center">
      <span className="flex h-12 w-12 items-center justify-center rounded-full bg-green-500/15 text-green-400">
        <svg
          viewBox="0 0 24 24"
          className="h-6 w-6"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.5"
          aria-hidden
        >
          <path d="M20 6 9 17l-5-5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </span>
      <span className="text-sm font-medium text-fg">{message}</span>
    </div>
  );
}
