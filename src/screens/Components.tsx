import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/Button";
import { ErrorNote } from "@/components/ErrorNote";
import { PageHeader, Section } from "@/components/Section";
import { useApp } from "@/store/app";
import { commands, toErrorDto } from "@/lib/commands";
import type { ComponentRow } from "@/lib/generated/ComponentRow";
import type { FileInfo } from "@/lib/generated/FileInfo";
import type { ErrorDto } from "@/lib/generated/ErrorDto";
import type { Settings } from "@/lib/generated/Settings";
import { t } from "@/i18n";

type UserFileKey = "model_path" | "renodx_addon_path" | "dlss_runtime_path";

const userFiles: { key: UserFileKey; label: "components.user.model" | "components.user.renodx" | "components.user.runtime"; hint: "components.user.modelHint" | "components.user.renodxHint" | "components.user.runtimeHint"; filter: string }[] = [
  { key: "model_path", label: "components.user.model", hint: "components.user.modelHint", filter: "nvngx_dlssnr.dll" },
  { key: "renodx_addon_path", label: "components.user.renodx", hint: "components.user.renodxHint", filter: "renodx-dlss5.addon64" },
  { key: "dlss_runtime_path", label: "components.user.runtime", hint: "components.user.runtimeHint", filter: "nvngx_dlss.dll" },
];

export function Components() {
  const { components, loadComponents, settings } = useApp();

  useEffect(() => {
    if (components === null) void loadComponents();
  }, [components, loadComponents]);

  return (
    <div className="px-8 py-6">
      <PageHeader title={t("components.title")} />
      <p className="mb-6 max-w-2xl text-sm text-text-2">{t("components.body")}</p>

      <div className="flex flex-col gap-4">
        <Section title={t("components.model.title")} description={t("components.model.body")}>
          <div className="divide-y divide-border">
            {settings && userFiles.map((f) => <UserFileRow key={f.key} spec={f} settings={settings} />)}
          </div>
        </Section>

        <div className="rounded-lg border border-border bg-surface shadow-[var(--shadow)]">
          {(components ?? []).map((row, i) => (
            <ComponentLine key={row.component.id} row={row} first={i === 0} />
          ))}
          {components?.length === 0 && (
            <p className="px-5 py-3 text-sm text-text-3">{t("components.status.missing")}</p>
          )}
        </div>
      </div>
    </div>
  );
}

function ComponentLine({ row, first }: { row: ComponentRow; first: boolean }) {
  const progress = useApp((s) => s.componentProgress[row.component.id]);
  const error = useApp((s) => s.componentErrors[row.component.id]);
  const fetchComponent = useApp((s) => s.fetchComponent);
  const c = row.component;
  const busy = progress !== undefined;

  const status = (() => {
    switch (row.status.kind) {
      case "verified":
        return { text: t("components.status.verified"), cls: "text-success" };
      case "missing":
        return { text: t("components.status.missing"), cls: "text-text-3" };
      case "mismatch":
        return { text: t("components.status.mismatch", { detail: row.status.detail }), cls: "text-warning" };
    }
  })();

  return (
    <div className={["px-5 py-3", first ? "" : "border-t border-border"].join(" ")}>
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div className="text-sm font-medium">{c.name}</div>
          <div className="truncate text-xs text-text-3">
            <button
              type="button"
              onClick={() => void openUrl(c.publisher)}
              className="underline-offset-2 hover:underline"
            >
              {c.publisher.replace("https://", "")}
            </button>
            {" · "}
            {t("components.pinned", { version: c.version, license: c.license })}
          </div>
          <div className="mt-1 text-xs text-text-2">{c.summary}</div>
        </div>
        <div className="flex shrink-0 items-center gap-3">
          <span className={`text-xs ${status.cls}`}>{status.text}</span>
          <Button
            variant={row.status.kind === "verified" ? "secondary" : "primary"}
            disabled={busy}
            onClick={() => void fetchComponent(c.id)}
          >
            {busy
              ? t("components.fetching", {
                  pct: progress.expected ? Math.floor((progress.received / progress.expected) * 100) : 0,
                })
              : row.status.kind === "verified"
                ? t("components.refetch")
                : t("components.fetch")}
          </Button>
        </div>
      </div>
      {busy && (
        <div className="mt-2 h-1 w-full overflow-hidden rounded-full bg-surface-2">
          <div
            className="h-full bg-accent transition-[width]"
            style={{ width: `${progress.expected ? (progress.received / progress.expected) * 100 : 0}%` }}
          />
        </div>
      )}
      {error && (
        <div className="mt-2">
          <ErrorNote error={error} />
        </div>
      )}
    </div>
  );
}

function UserFileRow({ spec, settings }: { spec: (typeof userFiles)[number]; settings: Settings }) {
  const updateSettings = useApp((s) => s.updateSettings);
  const path = settings[spec.key];
  const [info, setInfo] = useState<FileInfo | null>(null);
  const [error, setError] = useState<ErrorDto | null>(null);

  useEffect(() => {
    let live = true;
    setInfo(null);
    setError(null);
    if (!path) return;
    commands
      .inspectFile(path)
      .then((i) => live && setInfo(i))
      .catch((e) => live && setError(toErrorDto(e)));
    return () => {
      live = false;
    };
  }, [path]);

  const choose = async () => {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: spec.filter, extensions: [spec.filter.split(".").pop() ?? "*"] }],
    });
    if (typeof picked === "string") {
      setError(await updateSettings({ [spec.key]: picked } as Partial<Settings>));
    }
  };

  return (
    <div className="flex items-start justify-between gap-4 py-3">
      <div className="min-w-0">
        <div className="text-sm font-medium">{t(spec.label)}</div>
        <div className="text-xs text-text-3">{t(spec.hint)}</div>
        {path ? (
          <div className="mt-1 text-xs">
            <div className="select-text truncate text-text-2" title={path}>
              {path}
            </div>
            {info && (
              <div className="text-text-3">
                {formatSize(info.size)} · <span className="select-text font-mono">{info.sha256.slice(0, 16)}…</span>
                {" · "}
                <span className={signatureClass(info)}>{signatureText(info)}</span>
              </div>
            )}
            {error && (
              <div className="mt-1">
                <ErrorNote error={error} />
              </div>
            )}
          </div>
        ) : (
          <div className="mt-1 text-xs text-text-3">{t("components.model.none")}</div>
        )}
      </div>
      <div className="flex shrink-0 gap-2">
        <Button onClick={() => void choose()}>{t("components.choose")}</Button>
        {path && (
          <Button onClick={() => void updateSettings({ [spec.key]: null } as Partial<Settings>)}>
            {t("components.clear")}
          </Button>
        )}
      </div>
    </div>
  );
}

export function signatureText(info: FileInfo): string {
  const s = info.signature;
  switch (s.kind) {
    case "valid":
      return t("components.signature.valid", { signer: s.signer });
    case "hash_mismatch":
      return t("components.signature.hash_mismatch", { signer: s.signer });
    case "unsigned":
      return t("components.signature.unsigned");
    case "unknown":
      return t("components.signature.unknown", { detail: s.detail });
  }
}

function signatureClass(info: FileInfo): string {
  switch (info.signature.kind) {
    case "valid":
      return "text-success";
    case "hash_mismatch":
      return "text-danger";
    default:
      return "text-text-3";
  }
}

export function formatSize(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(0)} kB`;
  return `${bytes} B`;
}
