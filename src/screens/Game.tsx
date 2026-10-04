import { useEffect, useState } from "react";
import { ArrowLeft } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { SPONSOR_URL } from "@/lib/links";
import { Button } from "@/components/Button";
import { EmptyState } from "@/components/EmptyState";
import { ErrorNote } from "@/components/ErrorNote";
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
import type { Preview } from "@/lib/generated/Preview";
import type { InstallManifest } from "@/lib/generated/InstallManifest";
import type { ChangeKind } from "@/lib/generated/ChangeKind";
import type { InstallMode } from "@/lib/generated/InstallMode";
import type { LastRun } from "@/lib/generated/LastRun";
import type { ErrorDto } from "@/lib/generated/ErrorDto";
import { t } from "@/i18n";

const ANTI_CHEAT_PHRASE = "REMOVE-MY-DOUBTS";

export function Game({ id }: { id: string }) {
  const go = useApp((s) => s.go);
  const game = useApp((s) => s.library.games.find((g) => g.id === id));
  const clearOutcome = useApp((s) => s.clearOutcome);

  useEffect(() => () => clearOutcome(id), [id, clearOutcome]);

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
  const installed = game.status.kind === "installed" || game.status.kind === "update_available";

  return (
    <div className="px-8 py-6">
      <div className="mb-6">{back}</div>

      <div className="mb-6 flex items-start gap-6">
        <div className="aspect-[2/3] w-40 shrink-0 overflow-hidden rounded-md border border-border bg-surface-2">
          {game.cover && (
            <img src={convertFileSrc(game.cover)} alt="" className="size-full object-cover" />
          )}
        </div>
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
        {installed ? <InstalledSection game={game} /> : game.status.kind === "ready" && <ChangesSection game={game} />}
        {installed && <LastRunSection game={game} />}
        {game.analysis?.ships_dlss &&
          (game.analysis.apis.includes("dx12") || game.analysis.apis.includes("dx11")) && (
            <AdvancedSection game={game} locked={installed} />
          )}
      </div>
    </div>
  );
}

/** The one sentence, the primary button, and the inline progress/result. */
function RouteSection({ game }: { game: GameModel }) {
  const s = game.status;
  const installing = useApp((st) => st.installing);
  const progress = useApp((st) => st.installProgress);
  const outcome = useApp((st) => st.installOutcome[game.id]);
  const install = useApp((st) => st.install);
  const remove = useApp((st) => st.remove);
  const update = useApp((st) => st.update);
  const openFolder = useApp((st) => st.openFolder);
  const relaunchElevated = useApp((st) => st.relaunchElevated);
  const [confirm, setConfirm] = useState<"none" | "remove" | "anti_cheat">("none");
  const [phrase, setPhrase] = useState("");
  const [blocker, setBlocker] = useState<Preview["blocker"]>(null);

  useEffect(() => {
    setConfirm("none");
    setPhrase("");
  }, [game.id, game.status.kind]);

  // Learn up front whether an install can start, so the button can say why not.
  useEffect(() => {
    let live = true;
    setBlocker(null);
    if (s.kind !== "ready") return;
    commands
      .planPreview(game.id)
      .then((p) => live && setBlocker(p.blocker))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [game.id, s.kind]);

  if (s.kind === "pending") {
    return <Section title={t("game.pendingTitle")} description={t("game.pendingBody")}>{null}</Section>;
  }

  const busyHere = installing === game.id;
  const antiCheat = game.analysis?.anti_cheat ?? null;
  let sentence: string;
  switch (s.kind) {
    case "ready":
      sentence = s.reason;
      break;
    case "installed":
    case "update_available":
      sentence = `${routeLabel[s.route]}.`;
      break;
    case "anti_cheat":
      sentence = t("game.route.antiCheat", { name: antiCheatLabel[s.which] });
      break;
    case "unsupported":
      sentence = t("game.route.unsupported", { reason: s.reason });
      break;
  }

  const startInstall = () => {
    if (antiCheat) {
      setConfirm("anti_cheat");
    } else {
      void install(game.id, false);
    }
  };

  return (
    <Section title={t("game.route.title")} description={sentence}>
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-3">
          {(s.kind === "ready" || s.kind === "anti_cheat") && (
            <Button
              variant="primary"
              disabled={busyHere || installing !== null || (s.kind === "ready" && blocker !== null)}
              onClick={startInstall}
            >
              {busyHere ? t("game.installing") : t("game.install")}
            </Button>
          )}
          {s.kind === "update_available" && (
            <Button
              variant="primary"
              disabled={busyHere || installing !== null}
              onClick={() => void update(game.id)}
            >
              {busyHere ? t("game.installing") : t("game.update")}
            </Button>
          )}
          {(s.kind === "installed" || s.kind === "update_available") && (
            <Button
              variant="danger"
              disabled={busyHere || installing !== null}
              onClick={() => setConfirm("remove")}
            >
              {busyHere ? t("game.removing") : t("game.remove")}
            </Button>
          )}
          <Button onClick={() => void openFolder(game.id)}>{t("game.openFolder")}</Button>
          {s.kind === "ready" && blocker && (
            <span className="text-xs text-warning">{t("game.notReady", { message: blocker.message })}</span>
          )}
        </div>

        {busyHere && progress && (
          <div className="text-xs text-text-2" aria-live="polite">
            {progress.total > 0 ? `${progress.index + 1}/${progress.total} · ` : ""}
            {progress.message}
          </div>
        )}

        {confirm === "remove" && (
          <RemoveConfirm
            gameId={game.id}
            onCancel={() => setConfirm("none")}
            onConfirm={() => {
              setConfirm("none");
              void remove(game.id);
            }}
          />
        )}

        {confirm === "anti_cheat" && antiCheat && (
          <div className="rounded-md border border-danger/40 bg-danger/5 p-4 text-sm">
            <div className="font-medium">{t("game.antiCheat.title", { name: antiCheatLabel[antiCheat] })}</div>
            <p className="mt-1 text-text-2">{t("game.antiCheat.body")}</p>
            <input
              type="text"
              value={phrase}
              onChange={(e) => setPhrase(e.target.value)}
              placeholder={t("game.antiCheat.placeholder")}
              aria-label={t("game.antiCheat.placeholder")}
              className="mt-3 h-9 w-72 rounded-md border border-border bg-surface px-3 text-sm outline-none focus:border-accent"
            />
            <div className="mt-3 flex gap-2">
              <Button
                variant="danger"
                disabled={phrase.trim() !== ANTI_CHEAT_PHRASE}
                onClick={() => {
                  setConfirm("none");
                  void install(game.id, true);
                }}
              >
                {t("game.antiCheat.yes")}
              </Button>
              <Button onClick={() => setConfirm("none")}>{t("game.cancel")}</Button>
            </div>
          </div>
        )}

        {outcome && <Outcome outcome={outcome} />}
        {outcome?.kind === "failed" && outcome.error.code === "game_folder_not_writable" && (
          <div className="flex flex-wrap items-center gap-3 text-xs text-text-2">
            <span>{t("game.elevateHint")}</span>
            <Button onClick={() => void relaunchElevated()}>{t("game.elevate")}</Button>
          </div>
        )}
      </div>
    </Section>
  );
}

const modes: { mode: InstallMode; label: "game.mode.dlss5" | "game.mode.optiscaler_dlss5" | "game.mode.optiscaler_only"; hint: "game.mode.dlss5Hint" | "game.mode.optiscaler_dlss5Hint" | "game.mode.optiscaler_onlyHint" }[] = [
  { mode: "dlss5", label: "game.mode.dlss5", hint: "game.mode.dlss5Hint" },
  { mode: "opti_scaler_dlss5", label: "game.mode.optiscaler_dlss5", hint: "game.mode.optiscaler_dlss5Hint" },
  { mode: "opti_scaler_only", label: "game.mode.optiscaler_only", hint: "game.mode.optiscaler_onlyHint" },
];

/** Route choice for games that ship DLSS. */
function AdvancedSection({ game, locked }: { game: GameModel; locked: boolean }) {
  const setMode = useApp((s) => s.setMode);
  const setNeural = useApp((s) => s.setNeural);
  const [error, setError] = useState<ErrorDto | null>(null);
  const current: InstallMode = game.mode ?? "dlss5";
  return (
    <Section title={t("game.advanced.title")} description={t("game.advanced.body")}>
      <div className="flex flex-col gap-2" role="radiogroup" aria-label={t("game.advanced.title")}>
        {modes.map((m) => (
          <label
            key={m.mode}
            className={[
              "flex cursor-pointer items-start gap-3 rounded-md border px-3 py-2",
              current === m.mode ? "border-accent bg-surface-2" : "border-border",
              locked ? "cursor-not-allowed opacity-60" : "",
            ].join(" ")}
          >
            <input
              type="radio"
              name={`mode-${game.id}`}
              value={m.mode}
              checked={current === m.mode}
              disabled={locked}
              onChange={() => void setMode(game.id, m.mode === "dlss5" ? null : m.mode).then(setError)}
              className="mt-1"
            />
            <span>
              <span className="block text-sm">{t(m.label)}</span>
              <span className="block text-xs text-text-3">{t(m.hint)}</span>
            </span>
          </label>
        ))}
      </div>
      {current === "opti_scaler_dlss5" && (
        <div className="mt-4">
          <div className="mb-2 text-sm font-medium">{t("game.placement.title")}</div>
          <div className="flex flex-col gap-2" role="radiogroup" aria-label={t("game.placement.title")}>
            {(
              [
                { before: true, label: "game.placement.before", hint: "game.placement.beforeHint" },
                { before: false, label: "game.placement.after", hint: "game.placement.afterHint" },
              ] as const
            ).map((p) => {
              const selected = (game.neural?.before_upscale ?? true) === p.before;
              return (
                <label
                  key={String(p.before)}
                  className={[
                    "flex cursor-pointer items-start gap-3 rounded-md border px-3 py-2",
                    selected ? "border-accent bg-surface-2" : "border-border",
                    locked ? "cursor-not-allowed opacity-60" : "",
                  ].join(" ")}
                >
                  <input
                    type="radio"
                    name={`placement-${game.id}`}
                    checked={selected}
                    disabled={locked}
                    onChange={() =>
                      void setNeural(game.id, p.before ? null : { before_upscale: false }).then(setError)
                    }
                    className="mt-1"
                  />
                  <span>
                    <span className="block text-sm">{t(p.label)}</span>
                    <span className="block text-xs text-text-3">{t(p.hint)}</span>
                  </span>
                </label>
              );
            })}
          </div>
        </div>
      )}
      {locked && <p className="mt-2 text-xs text-text-3">{t("game.advanced.locked")}</p>}
      {error && (
        <div className="mt-2">
          <ErrorNote error={error} />
        </div>
      )}
    </Section>
  );
}

/** What the component log beside the exe said after the last run. */
function LastRunSection({ game }: { game: GameModel }) {
  const [run, setRun] = useState<LastRun | null | undefined>(undefined);
  useEffect(() => {
    let live = true;
    commands
      .lastRun(game.id)
      .then((r) => live && setRun(r))
      .catch(() => live && setRun(null));
    return () => {
      live = false;
    };
  }, [game.id, game.status]);
  if (run === undefined) return null;
  return (
    <Section title={t("game.lastRun.title")} description={t("game.lastRun.body")}>
      {run === null ? (
        <p className="text-sm text-text-3">{t("game.lastRun.none")}</p>
      ) : (
        <>
          <p className={`mb-2 text-sm ${run.failed ? "text-danger" : "text-success"}`}>
            {run.failed ? t("game.lastRun.failed") : t("game.lastRun.ok")}
            <span className="ml-2 text-xs text-text-3">
              {relative(game.install_dir, run.log)} · {new Date(run.modified * 1000).toLocaleString()}
            </span>
          </p>
          <pre className="max-h-64 select-text overflow-auto whitespace-pre-wrap rounded-md bg-surface-2 p-3 font-mono text-xs text-text-2">
            {run.lines.join("\n")}
          </pre>
        </>
      )}
    </Section>
  );
}

function Outcome({ outcome }: { outcome: NonNullable<ReturnType<typeof useApp.getState>["installOutcome"][string]> }) {
  switch (outcome.kind) {
    case "installed":
      return (
        <div className="flex flex-wrap items-center gap-3">
          <p className="text-sm text-success">{t("game.installDone", { n: outcome.files })}</p>
          <span className="text-xs text-text-3">{t("support.afterInstall")}</span>
          <Button onClick={() => void openUrl(SPONSOR_URL)}>{t("support.coffee")}</Button>
        </div>
      );
    case "removed":
      return <p className="text-sm text-success">{t("game.removeDone")}</p>;
    case "failed":
      return (
        <div className="flex flex-col gap-2">
          <p className="text-sm text-text-2">{t("game.installFailed")}</p>
          <ErrorNote error={outcome.error} />
        </div>
      );
  }
}

function RemoveConfirm({
  gameId,
  onCancel,
  onConfirm,
}: {
  gameId: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const [manifest, setManifest] = useState<InstallManifest | null>(null);
  useEffect(() => {
    let live = true;
    commands
      .getInstall(gameId)
      .then((m) => live && setManifest(m))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [gameId]);
  return (
    <div className="rounded-md border border-border bg-surface-2 p-4 text-sm">
      <div className="font-medium">{t("game.removeConfirm.title")}</div>
      <p className="mt-1 text-text-2">{t("game.removeConfirm.body", { n: manifest?.files.length ?? "…" })}</p>
      <div className="mt-3 flex gap-2">
        <Button variant="danger" onClick={onConfirm}>
          {t("game.removeConfirm.yes")}
        </Button>
        <Button onClick={onCancel}>{t("game.cancel")}</Button>
      </div>
    </div>
  );
}

const kindKey: Record<ChangeKind, "game.changes.add" | "game.changes.backup" | "game.changes.edit"> = {
  add: "game.changes.add",
  backup: "game.changes.backup",
  edit: "game.changes.edit",
};

/** The planned file list, fetched from the core planner. */
function ChangesSection({ game }: { game: GameModel }) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const settings = useApp((s) => s.settings);

  useEffect(() => {
    let live = true;
    commands
      .planPreview(game.id)
      .then((p) => live && setPreview(p))
      .catch(() => live && setPreview({ changes: [], exact: false, blocker: null }));
    return () => {
      live = false;
    };
  }, [game.id, game.status, settings]);

  return (
    <Section title={t("game.changes.title")} description={t("game.changes.body")}>
      {preview === null ? null : preview.changes.length === 0 ? (
        <p className="text-sm text-text-2">{t("game.changes.none")}</p>
      ) : (
        <>
          <p className="mb-2 text-xs text-text-3">{preview.exact ? t("game.exactPlan") : t("game.sketchPlan")}</p>
          <ChangeList changes={preview.changes} root={game.install_dir} />
        </>
      )}
    </Section>
  );
}

/** What is installed right now, straight from the manifest. */
function InstalledSection({ game }: { game: GameModel }) {
  const [manifest, setManifest] = useState<InstallManifest | null>(null);
  useEffect(() => {
    let live = true;
    commands
      .getInstall(game.id)
      .then((m) => live && setManifest(m))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [game.id, game.status]);
  if (!manifest) return null;
  const changes = manifest.files.map((f) => ({
    kind: (f.backup ? "backup" : "add") as ChangeKind,
    path: f.target,
    note: f.backup ? `backup: ${f.backup}` : `sha256 ${f.sha256_after.slice(0, 16)}…`,
  }));
  return (
    <Section
      title={t("game.changes.title")}
      description={t("game.installed", {
        route: routeLabel[manifest.route],
        date: new Date(manifest.installed_at * 1000).toLocaleString(),
        n: manifest.files.length,
      })}
    >
      <ChangeList changes={changes} root={game.install_dir} />
    </Section>
  );
}

function ChangeList({ changes, root }: { changes: { kind: ChangeKind; path: string; note: string }[]; root: string }) {
  return (
    <ul className="divide-y divide-border text-sm">
      {changes.map((c) => (
        <li key={c.path} className="grid grid-cols-[9rem_1fr] gap-3 py-2">
          <span className={c.kind === "add" ? "text-text-2" : "text-warning"}>{t(kindKey[c.kind])}</span>
          <span className="min-w-0">
            <span className="block select-text truncate font-mono text-xs" title={c.path}>
              {relative(root, c.path)}
            </span>
            <span className="block truncate text-xs text-text-3" title={c.note}>
              {c.note}
            </span>
          </span>
        </li>
      ))}
    </ul>
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
