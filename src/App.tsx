import { useEffect } from "react";
import { Rail } from "@/components/Rail";
import { Library } from "@/screens/Library";
import { Game } from "@/screens/Game";
import { Components } from "@/screens/Components";
import { Settings } from "@/screens/Settings";
import { useApp } from "@/store/app";
import { applyTheme, watchSystemTheme } from "@/lib/theme";
import { setLanguage } from "@/i18n";
import { ErrorNote } from "@/components/ErrorNote";
import { Button } from "@/components/Button";

export function App() {
  const screen = useApp((s) => s.screen);
  const boot = useApp((s) => s.boot);
  const theme = useApp((s) => s.settings?.theme ?? "system");
  const language = useApp((s) => s.settings?.language ?? "en");
  const bootError = useApp((s) => s.bootError);
  const info = useApp((s) => s.info);
  const warnings = info?.startup_warnings ?? [];

  useEffect(() => {
    void boot();
  }, [boot]);

  useEffect(() => {
    setLanguage(language);
  }, [language]);

  useEffect(() => {
    applyTheme(theme);
    return watchSystemTheme(() => applyTheme(theme));
  }, [theme]);

  return (
    <div className="flex h-full">
      <Rail />
      <main className="min-w-0 flex-1 overflow-y-auto">
        {(bootError || warnings.length > 0) && (
          <div className="flex flex-col gap-2 px-8 pt-6">
            {bootError && (
              <div className="flex items-start gap-3">
                <div className="flex-1">
                  <ErrorNote error={bootError} />
                </div>
                <Button onClick={() => void boot()}>Retry</Button>
              </div>
            )}
            {warnings.map((w, i) => (
              <ErrorNote key={i} error={w} />
            ))}
          </div>
        )}
        {screen.kind === "library" && <Library />}
        {screen.kind === "game" && <Game id={screen.id} />}
        {screen.kind === "components" && <Components />}
        {screen.kind === "settings" && <Settings />}
      </main>
    </div>
  );
}
