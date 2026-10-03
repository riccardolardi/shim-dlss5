import { Gamepad2, Package, Settings as SettingsIcon } from "lucide-react";
import { useApp, type Screen } from "@/store/app";
import { t } from "@/i18n";

const items: { screen: Screen; label: "nav.library" | "nav.components" | "nav.settings"; Icon: typeof Gamepad2 }[] = [
  { screen: { kind: "library" }, label: "nav.library", Icon: Gamepad2 },
  { screen: { kind: "components" }, label: "nav.components", Icon: Package },
  { screen: { kind: "settings" }, label: "nav.settings", Icon: SettingsIcon },
];

export function Rail() {
  const screen = useApp((s) => s.screen);
  const go = useApp((s) => s.go);
  const version = useApp((s) => s.info?.version);

  const activeKind = screen.kind === "game" ? "library" : screen.kind;

  return (
    <nav className="flex w-[200px] shrink-0 flex-col border-r border-border bg-surface px-3 py-4">
      <div className="mb-6 flex items-center gap-2 px-2">
        <span className="inline-block size-2.5 rounded-full bg-accent" aria-hidden />
        <span className="text-base font-semibold tracking-tight">{t("app.name")}</span>
      </div>

      <ul className="flex flex-col gap-1">
        {items.map(({ screen: target, label, Icon }) => {
          const active = target.kind === activeKind;
          return (
            <li key={target.kind}>
              <button
                type="button"
                onClick={() => go(target)}
                aria-current={active ? "page" : undefined}
                className={[
                  "flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm transition-colors",
                  active
                    ? "bg-surface-2 font-medium text-text"
                    : "text-text-2 hover:bg-surface-2 hover:text-text",
                ].join(" ")}
              >
                <Icon size={16} strokeWidth={1.75} aria-hidden />
                {t(label)}
              </button>
            </li>
          );
        })}
      </ul>

      <div className="mt-auto px-2 text-xs text-text-3">{version ? `v${version}` : ""}</div>
    </nav>
  );
}
