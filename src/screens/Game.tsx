import { useEffect, useState } from "react";
import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/Button";
import { EmptyState } from "@/components/EmptyState";
import { Section } from "@/components/Section";
import { useApp } from "@/store/app";
import { commands } from "@/lib/commands";
import {
  antiCheatLabel,
  apiLabel,
  bitnessLabel,
  engineLabel,
  routeLabel,
  statusLine,
  type Tone,
} from "@/lib/status";
import type { Game as GameModel } from "@/lib/generated/Game";
import type { Analysis } from "@/lib/generated/Analysis";
import type { PlannedChange } from "@/lib/generated/PlannedChange";
import type { ChangeKind } from "@/lib/generated/ChangeKind";
import { t } from "@/i18n";

export function Game({ id }: { id: string }) {
  const go = useApp((s) => s.go);
  const game = useApp((s) => s.library.games.find((g) => g.id === id));

  const back = (
    <Button onClick={() => go({ kind: "library" })}>
      <ArrowLeft size={14} aria-hidden />
      {t("game.back")}
    </Button>
  );

  if (!game) {
    return (
      <div className="px-8 py-6">
        <div className="mb-6">{back}</div>
        <EmptyState title={t("game.notFound")} body="" />
      </div>
    );
  }

  const status = statusLine(game.status);

  return (
    <div className="px-8 py-6">
      <div className="mb-6">{back}</div>

      <div className="mb-6 flex items-start gap-6">
        <div className="aspect-[2/3] w-40 shrink-0 overflow-hidden rounded-md border border-border bg-surface-2" />
        <div className="min-w-0 flex-1">
          <h1 className="text-xl font-semibold tracking-tight">{game.title}</h1>
          <div className="mt-2 flex flex-wrap gap-1.5">
            <Badge>{t(`launcher.${game.launcher}` as const)}</Badge>
            <Badge tone={status.tone}>{status.text}</Badge>
            {game.analysis && <FactBadges analysis={game.analysis} />}
          </div>
          <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs text-text-3">
            <dt>{t("game.installDir")}</dt>
            <dd className="select-text truncate text-text-2" title={game.install_dir}>
              {game.install_dir}
            </dd>
            {game.analysis && (
              <>
                <dt>{t("game.exe")}</dt>
                <dd className="select-text truncate text-text-2" title={game.analysis.exe}>
                  {relative(game.install_dir, game.analysis.exe)}
                </dd>
              </>
            )}
          </dl>
        </div>
      </div>

      <div className="flex flex-col gap-4">
        <RouteSection game={game} />
        {game.status.kind === "ready" && <ChangesSection game={game} />}
      </div>
    </div>
  );
}

/** The one sentence: what shim would do and why. */
function RouteSection({ game }: { game: GameModel }) {
  const s = game.status;
  if (s.kind === "pending") {
    return <Section title={t("game.pendingTitle")} description={t("game.pendingBody")}>{null}</Section>;
  }
  let sentence: string;
  switch (s.kind) {
    case "ready":
    case "installed":
    case "update_available":
      sentence = s.kind === "ready" ? s.reason : `${routeLabel[s.route]}.`;
      break;
    case "anti_cheat":
      sentence = t("game.route.antiCheat", { name: antiCheatLabel[s.which] });
      break;
    case "unsupported":
      sentence = t("game.route.unsupported", { reason: s.reason });
      break;
  }
  return (
    <Section title={t("game.route.title")} description={sentence}>
      <div className="flex items-center gap-3">
        <Button variant="primary" disabled title={t("game.installSoon")}>
          {t("game.install")}
        </Button>
        <Button disabled>{t("game.openFolder")}</Button>
        <span className="text-xs text-text-3">{t("game.installSoon")}</span>
      </div>
    </Section>
  );
}

const kindKey: Record<ChangeKind, "game.changes.add" | "game.changes.backup" | "game.changes.edit"> = {
  add: "game.changes.add",
  backup: "game.changes.backup",
  edit: "game.changes.edit",
};

/** The planned file list, fetched from the core planner. */
function ChangesSection({ game }: { game: GameModel }) {
  const [plan, setPlan] = useState<PlannedChange[] | null>(null);

  useEffect(() => {
    let live = true;
    commands
      .planPreview(game.id)
      .then((p) => live && setPlan(p))
      .catch(() => live && setPlan([]));
    return () => {
      live = false;
    };
  }, [game.id, game.status]);

  return (
    <Section title={t("game.changes.title")} description={t("game.changes.body")}>
      {plan === null ? null : plan.length === 0 ? (
        <p className="text-sm text-text-2">{t("game.changes.none")}</p>
      ) : (
        <ul className="divide-y divide-border text-sm">
          {plan.map((c) => (
            <li key={c.path} className="grid grid-cols-[9rem_1fr] gap-3 py-2">
              <span className={c.kind === "add" ? "text-text-2" : "text-warning"}>{t(kindKey[c.kind])}</span>
              <span className="min-w-0">
                <span className="block select-text truncate font-mono text-xs" title={c.path}>
                  {relative(game.install_dir, c.path)}
                </span>
                <span className="block text-xs text-text-3">{c.note}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </Section>
  );
}

function FactBadges({ analysis: a }: { analysis: Analysis }) {
  const engine = engineLabel[a.engine];
  return (
    <>
      <Badge>{bitnessLabel[a.bitness]}</Badge>
      {a.apis.map((api) => (
        <Badge key={api}>{apiLabel[api]}</Badge>
      ))}
      {engine && <Badge>{engine}</Badge>}
      <Badge>
        {a.ships_dlss
          ? a.dlss_version
            ? t("game.facts.dlss", { version: a.dlss_version })
            : t("game.facts.dlssUnknown")
          : t("game.facts.noDlss")}
      </Badge>
      {a.has_dlss5_model && <Badge tone="success">{t("game.facts.model")}</Badge>}
      {a.foreign_reshade && <Badge tone="warning">{t("game.facts.foreignReshade")}</Badge>}
      {a.foreign_optiscaler && <Badge tone="warning">{t("game.facts.foreignOptiscaler")}</Badge>}
      {a.anti_cheat && <Badge tone="danger">{antiCheatLabel[a.anti_cheat]}</Badge>}
    </>
  );
}

const badgeTone: Record<Tone, string> = {
  neutral: "border-border bg-surface-2 text-text-2",
  success: "border-success/40 bg-success/10 text-success",
  warning: "border-warning/40 bg-warning/10 text-warning",
  danger: "border-danger/40 bg-danger/10 text-danger",
};

function Badge({ children, tone = "neutral" }: { children: string; tone?: Tone }) {
  return (
    <span className={`rounded-sm border px-2 py-0.5 text-xs ${badgeTone[tone]}`}>{children}</span>
  );
}

/** `C:\Games\X\bin\x.exe` under `C:\Games\X` → `bin\x.exe`. */
export function relative(root: string, path: string): string {
  const norm = (s: string) => s.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
  const r = norm(root);
  const p = norm(path);
  return p.startsWith(r + "\\") ? path.slice(r.length + 1) : path;
}
