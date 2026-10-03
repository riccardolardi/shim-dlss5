import { PageHeader, Section } from "@/components/Section";
import { t } from "@/i18n";

/** Pinned components. Real rows (version, hash status, update) arrive in Phase 2. */
const planned = [
  { id: "optiscaler", name: "OptiScaler", publisher: "optiscaler/OptiScaler", license: "GPL-3.0" },
  { id: "reshade", name: "ReShade (add-on build)", publisher: "reshade.me", license: "BSD-3" },
  { id: "renodx-dlss5", name: "RenoDX DLSS 5 add-on", publisher: "clshortfuse/renodx", license: "MIT" },
  { id: "dlss5-feeder", name: "DLSS5-Feeder add-on", publisher: "jlrouzies-fr/DLSS5-Feeder", license: "MIT" },
];

export function Components() {
  return (
    <div className="px-8 py-6">
      <PageHeader title={t("components.title")} />
      <p className="mb-6 max-w-2xl text-sm text-text-2">{t("components.body")}</p>

      <div className="flex flex-col gap-4">
        <Section title={t("components.model.title")} description={t("components.model.body")}>
          <p className="text-sm text-text-3">{t("components.model.none")}</p>
        </Section>

        <div className="rounded-lg border border-border bg-surface shadow-[var(--shadow)]">
          {planned.map((c, i) => (
            <div
              key={c.id}
              className={[
                "flex items-center justify-between gap-4 px-5 py-3",
                i > 0 ? "border-t border-border" : "",
              ].join(" ")}
            >
              <div className="min-w-0">
                <div className="text-sm font-medium">{c.name}</div>
                <div className="truncate text-xs text-text-3">
                  {c.publisher} · {c.license}
                </div>
              </div>
              <span className="shrink-0 text-xs text-text-3">{t("components.comingSoon")}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
