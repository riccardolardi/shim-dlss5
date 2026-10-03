import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/Button";
import { EmptyState } from "@/components/EmptyState";
import { Section } from "@/components/Section";
import { useApp } from "@/store/app";
import { statusLine } from "@/lib/status";
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
        <div className="min-w-0">
          <h1 className="text-xl font-semibold tracking-tight">{game.title}</h1>
          <div className="mt-2 flex flex-wrap gap-1.5">
            <Badge>{t(`launcher.${game.launcher}` as const)}</Badge>
            <Badge>{status.text}</Badge>
          </div>
          <p className="mt-3 text-xs text-text-3">
            {t("game.installDir")}: <span className="select-text text-text-2">{game.install_dir}</span>
          </p>
        </div>
      </div>

      <Section title={t("game.pendingTitle")} description={t("game.pendingBody")}>
        <div className="flex gap-2">
          <Button disabled>{t("game.openFolder")}</Button>
        </div>
      </Section>
    </div>
  );
}

function Badge({ children }: { children: string }) {
  return (
    <span className="rounded-sm border border-border bg-surface-2 px-2 py-0.5 text-xs text-text-2">
      {children}
    </span>
  );
}
