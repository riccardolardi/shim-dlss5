import { useApp } from "@/store/app";
import { modelNote, newerBuildNote, type NotePlace } from "@/lib/modelNote";
import type { Route } from "@/lib/generated/Route";
import type { Tone } from "@/lib/status";

const toneClass: Record<Tone, string> = {
  neutral: "border-border text-text-2",
  success: "border-success/40 text-success",
  warning: "border-warning/40 text-warning",
  danger: "border-danger/40 text-danger",
};

/** What to know about the user's DLSS 5 model: GPU fit, and a newer build's limits on `route`. */
export function ModelNote({ place, route, className = "" }: { place: NotePlace; route?: Route; className?: string }) {
  const status = useApp((s) => s.modelStatus);
  const notes = [modelNote(status, place), newerBuildNote(status, place, route)].filter((n) => n !== null);
  if (notes.length === 0) return null;
  return (
    <div className={`flex flex-col gap-2 ${className}`}>
      {notes.map((note) => (
        <p key={note.text} className={`rounded-md border px-3 py-2 text-xs ${toneClass[note.tone]}`} role="note">
          {note.text}
        </p>
      ))}
    </div>
  );
}
