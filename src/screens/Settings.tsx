import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
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
  const { settings, info, updateSettings } = useApp();
  const [error, setError] = useState<ErrorDto | null>(null);

  if (!settings) return null;

  const save = async (patch: Parameters<typeof updateSettings>[0]) => {
    setError(await updateSettings(patch));
  };

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
