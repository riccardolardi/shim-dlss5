import { useState } from "react";
import type { ErrorDto } from "@/lib/generated/ErrorDto";
import { t } from "@/i18n";

/** Inline error: the user message, with the technical detail behind a toggle. */
export function ErrorNote({ error, prefix }: { error: ErrorDto; prefix?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-md border border-danger/40 bg-danger/5 px-3 py-2 text-sm">
      <div className="flex items-start justify-between gap-3">
        <span>
          {prefix && <span className="font-medium">{prefix}: </span>}
          {error.message}
        </span>
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          className="shrink-0 text-xs text-text-2 underline-offset-2 hover:underline"
        >
          {open ? t("error.hide") : t("error.details")}
        </button>
      </div>
      {open && (
        <pre className="mt-2 overflow-x-auto whitespace-pre-wrap text-xs text-text-2">
          {error.detail}
        </pre>
      )}
    </div>
  );
}
