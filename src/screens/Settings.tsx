import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/Button";
import { ErrorNote } from "@/components/ErrorNote";
import { PageHeader, Section } from "@/components/Section";
import { Toggle } from "@/components/Toggle";
import { useApp } from "@/store/app";
import type { ErrorDto } from "@/lib/generated/ErrorDto";
import type { Theme } from "@/lib/generated/Theme";
import type { ScanSources } from "@/lib/generated/ScanSources";
import { t } from "@/i18n";

const REPO_URL = "https://github.com/riccardolardi/shim-dlss5";

const sources: (keyof Omit<ScanSources, "custom_folders">)[] = [
  "steam",
  "epic",
  "gog",
  "xbox",
  "ubisoft",
  "ea",
];

const themes: Theme[] = ["system", "light", "dark"];

export function Settings() {
  const { settings, info, updateSettings, library, setHidden } = useApp();
  const [error, setError] = useState<ErrorDto | null>(null);

  if (!settings) return null;

  const save = async (patch: Parameters<typeof updateSettings>[0]) => {
    setError(await updateSettings(patch));
  };

  const addFolder = async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string" && !settings.scan.custom_folders.includes(picked)) {
      await save({ scan: { ...settings.scan, custom_folders: [...settings.scan.custom_folders, picked] } });
    }
  };

  const hidden = library.games.filter((g) => g.hidden).sort((a, b) => a.title.localeCompare(b.title));

  return (
    <div className="px-8 py-6">
      <PageHeader title={t("settings.title")} />
      {error && (
        <div className="mb-4">
          <ErrorNote error={error} prefix={t("settings.saveFailed")} />
        </div>
      )}

      <div className="flex max-w-2xl flex-col gap-4">
        <Section title={t("settings.scan.title")} description={t("settings.scan.body")}>
          <div className="divide-y divide-border">
            {sources.map((key) => (
              <Toggle
                key={key}
                label={t(`launcher.${key}` as const)}
                checked={settings.scan[key]}
                onChange={(on) => void save({ scan: { ...settings.scan, [key]: on } })}
              />
            ))}
          </div>
        </Section>

        <Section title={t("settings.folders.title")} description={t("settings.folders.body")}>
          <div className="flex flex-col gap-2">
            {settings.scan.custom_folders.length === 0 && (
              <p className="text-sm text-text-3">{t("settings.folders.none")}</p>
            )}
            {settings.scan.custom_folders.map((folder) => (
              <div key={folder} className="flex items-center justify-between gap-3 text-sm">
                <span className="select-text truncate" title={folder}>
                  {folder}
                </span>
                <Button
                  onClick={() =>
                    void save({
                      scan: {
                        ...settings.scan,
                        custom_folders: settings.scan.custom_folders.filter((f) => f !== folder),
                      },
                    })
                  }
                >
                  {t("settings.folders.remove")}
                </Button>
              </div>
            ))}
            <div>
              <Button onClick={() => void addFolder()}>{t("settings.folders.add")}</Button>
            </div>
          </div>
        </Section>

        <Section title={t("settings.hidden.title")} description={t("settings.hidden.body")}>
          {hidden.length === 0 ? (
            <p className="text-sm text-text-3">{t("settings.hidden.none")}</p>
          ) : (
            <div className="divide-y divide-border">
              {hidden.map((g) => (
                <div key={g.id} className="flex items-center justify-between gap-3 py-2 text-sm">
                  <span className="truncate">{g.title}</span>
                  <Button onClick={() => void setHidden(g.id, false).then((e) => setError(e))}>
                    {t("settings.hidden.unhide")}
                  </Button>
                </div>
              ))}
            </div>
          )}
        </Section>

        <Section title={t("settings.appearance.title")}>
          <div className="flex items-center justify-between">
            <span className="text-sm">{t("settings.theme")}</span>
            <div className="flex gap-1" role="radiogroup" aria-label={t("settings.theme")}>
              {themes.map((th) => (
                <button
                  key={th}
                  type="button"
                  role="radio"
                  aria-checked={settings.theme === th}
                  onClick={() => void save({ theme: th })}
                  className={[
                    "h-8 rounded-md px-3 text-xs font-medium transition-colors",
                    settings.theme === th
                      ? "bg-text text-bg"
                      : "bg-surface-2 text-text-2 hover:text-text",
                  ].join(" ")}
                >
                  {t(`settings.theme.${th}` as const)}
                </button>
              ))}
            </div>
          </div>
        </Section>

        <Section title={t("settings.updates.title")} description={t("settings.updates.body")}>
          <Toggle
            label={t("settings.updates.check")}
            checked={settings.check_updates}
            onChange={(on) => void save({ check_updates: on })}
          />
        </Section>

        <Section title={t("settings.about.title")}>
          <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
            <dt className="text-text-2">{t("settings.about.version", { version: "" }).trim()}</dt>
            <dd>{info?.version ?? "…"}</dd>
            <dt className="text-text-2">{t("settings.about.platform")}</dt>
            <dd>{info?.platform ?? "…"}</dd>
            <dt className="text-text-2">{t("settings.about.data")}</dt>
            <dd className="select-text break-all">{info?.data_dir ?? "…"}</dd>
            <dt className="text-text-2">{t("settings.about.repo")}</dt>
            <dd>
              <button
                type="button"
                onClick={() => void openUrl(REPO_URL)}
                className="text-accent underline-offset-2 hover:underline"
              >
                {REPO_URL.replace("https://", "")}
              </button>
              <span className="text-text-3"> · {t("settings.about.license")}</span>
            </dd>
          </dl>
        </Section>
      </div>
    </div>
  );
}
