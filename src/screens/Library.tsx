import { useEffect, useMemo, useState } from "react";
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

type Menu = { gameId: string; x: number; y: number };

export function Library() {
  const { library, scanning, scanProgress, scan, lastScan, scanError, info, go } = useApp();
  const openFolder = useApp((s) => s.openFolder);
  const rescanGame = useApp((s) => s.rescanGame);
  const setHidden = useApp((s) => s.setHidden);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [menu, setMenu] = useState<Menu | null>(null);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", close);
    };
  }, [menu]);

  const progressText = !scanning
    ? null
    : scanProgress && scanProgress.total > 0 && scanProgress.title
      ? t("library.progress.analysing", {
          done: scanProgress.done + 1,
          total: scanProgress.total,
          title: scanProgress.title,
        })
      : t("library.progress.discovering");

  const hiddenCount = library.games.filter((g) => g.hidden).length;

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return library.games
      .filter((g) => !g.hidden)
      .filter((g) => matchesFilter(g.status, filter))
      .filter((g) => (q ? g.title.toLowerCase().includes(q) : true))
      .sort((a, b) => a.title.localeCompare(b.title));
  }, [library.games, query, filter]);

  const canScan = info?.can_scan ?? false;

  const menuItems: { label: string; run: (id: string) => void }[] = [
    { label: t("library.menu.open"), run: (id) => void openFolder(id) },
    { label: t("library.menu.rescan"), run: (id) => void rescanGame(id) },
    { label: t("library.menu.hide"), run: (id) => void setHidden(id, true) },
  ];

  return (
    <div className="px-8 py-6">
      <PageHeader title={t("library.title")}>
        <span className="max-w-xs truncate text-xs text-text-3" aria-live="polite">
          {progressText ??
            (library.scanned_at
              ? t("library.lastScan", { time: new Date(library.scanned_at * 1000).toLocaleString() })
              : t("library.neverScanned"))}
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
        {hiddenCount > 0 && (
          <button
            type="button"
            onClick={() => go({ kind: "settings" })}
            className="ml-auto text-xs text-text-3 underline-offset-2 hover:underline"
          >
            {t("library.hidden", { n: hiddenCount })}
          </button>
        )}
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
            <GameCard
              key={g.id}
              game={g}
              onOpen={() => go({ kind: "game", id: g.id })}
              onMenu={(x, y) => setMenu({ gameId: g.id, x, y })}
            />
          ))}
        </div>
      )}

      {menu && (
        <ul
          role="menu"
          className="fixed z-50 min-w-44 rounded-md border border-border bg-surface p-1 text-sm shadow-[var(--shadow)]"
          style={{ left: menu.x, top: menu.y }}
        >
          {menuItems.map((item) => (
            <li key={item.label} role="none">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  item.run(menu.gameId);
                  setMenu(null);
                }}
                className="w-full rounded px-2.5 py-1.5 text-left hover:bg-surface-2"
              >
                {item.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
