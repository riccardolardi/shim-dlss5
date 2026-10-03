import { useMemo, useState } from "react";
import { RefreshCw, Search } from "lucide-react";
import { Button } from "@/components/Button";
import { EmptyState } from "@/components/EmptyState";
import { ErrorNote } from "@/components/ErrorNote";
import { GameCard } from "@/components/GameCard";
import { PageHeader } from "@/components/Section";
import { useApp } from "@/store/app";
import { matchesFilter, type Filter } from "@/lib/status";
import { t } from "@/i18n";

const filters: Filter[] = ["all", "installed", "update", "anti_cheat", "unsupported"];

export function Library() {
  const { library, scanning, scan, lastScan, scanError, info, go } = useApp();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return library.games
      .filter((g) => !g.hidden)
      .filter((g) => matchesFilter(g.status, filter))
      .filter((g) => (q ? g.title.toLowerCase().includes(q) : true))
      .sort((a, b) => a.title.localeCompare(b.title));
  }, [library.games, query, filter]);

  const canScan = info?.can_scan ?? false;

  return (
    <div className="px-8 py-6">
      <PageHeader title={t("library.title")}>
        <span className="text-xs text-text-3">
          {library.scanned_at
            ? t("library.lastScan", { time: new Date(library.scanned_at * 1000).toLocaleString() })
            : t("library.neverScanned")}
        </span>
        <Button variant="primary" onClick={() => void scan()} disabled={scanning || !canScan}>
          <RefreshCw size={14} className={scanning ? "animate-spin" : ""} aria-hidden />
          {scanning ? t("library.scanning") : t("library.scan")}
        </Button>
      </PageHeader>

      <div className="mb-5 flex flex-wrap items-center gap-2">
        <label className="relative">
          <Search size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-text-3" aria-hidden />
          <input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("library.search")}
            aria-label={t("library.search")}
            className="h-9 w-64 rounded-md border border-border bg-surface pl-8 pr-3 text-sm outline-none focus:border-accent"
          />
        </label>
        <div className="flex gap-1">
          {filters.map((f) => (
            <button
              key={f}
              type="button"
              onClick={() => setFilter(f)}
              aria-pressed={filter === f}
              className={[
                "h-8 rounded-full px-3 text-xs font-medium transition-colors",
                filter === f ? "bg-text text-bg" : "bg-surface-2 text-text-2 hover:text-text",
              ].join(" ")}
            >
              {t(`filter.${f}` as const)}
            </button>
          ))}
        </div>
      </div>

      {scanError && (
        <div className="mb-5">
          <ErrorNote error={scanError} />
        </div>
      )}

      {lastScan && (lastScan.failures.length > 0 || lastScan.unavailable.length > 0) && (
        <div className="mb-5 flex flex-col gap-2">
          {lastScan.failures.map((f) => (
            <ErrorNote key={f.launcher} error={f.error} prefix={t(`launcher.${f.launcher}` as const)} />
          ))}
          {lastScan.unavailable.length > 0 && (
            <p className="text-xs text-text-3">
              {t("library.unavailable", {
                launchers: lastScan.unavailable.map((l) => t(`launcher.${l}` as const)).join(", "),
              })}
            </p>
          )}
        </div>
      )}

      {visible.length === 0 ? (
        <EmptyState
          title={t("library.empty.title")}
          body={
            canScan
              ? t("library.empty.body")
              : t("library.empty.noScan", { platform: info?.platform ?? "this OS" })
          }
          action={
            canScan ? (
              <Button variant="primary" onClick={() => void scan()} disabled={scanning}>
                {t("library.scan")}
              </Button>
            ) : undefined
          }
        />
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-5">
          {visible.map((g) => (
            <GameCard key={g.id} game={g} onOpen={() => go({ kind: "game", id: g.id })} />
          ))}
        </div>
      )}
    </div>
  );
}
