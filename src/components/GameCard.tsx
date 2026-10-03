import { convertFileSrc } from "@tauri-apps/api/core";
import type { Game } from "@/lib/generated/Game";
import { statusLine, type Tone } from "@/lib/status";

const toneClass: Record<Tone, string> = {
  neutral: "text-text-2",
  success: "text-success",
  warning: "text-warning",
  danger: "text-danger",
};

export function GameCard({ game, onOpen }: { game: Game; onOpen: () => void }) {
  const status = statusLine(game.status);
  return (
    <button
      type="button"
      onClick={onOpen}
      className="group flex w-full flex-col text-left"
      aria-label={game.title}
    >
      <div className="aspect-[2/3] w-full overflow-hidden rounded-md border border-border bg-surface-2 shadow-[var(--shadow)] transition group-hover:-translate-y-0.5">
        {game.cover ? (
          <img
            src={convertFileSrc(game.cover)}
            alt=""
            className="size-full object-cover"
            loading="lazy"
          />
        ) : (
          <div className="flex size-full items-center justify-center p-3 text-center text-sm font-medium text-text-2">
            {game.title}
          </div>
        )}
      </div>
      <div className="mt-2 truncate text-sm font-medium" title={game.title}>
        {game.title}
      </div>
      <div className={`truncate text-xs ${toneClass[status.tone]}`}>{status.text}</div>
    </button>
  );
}
