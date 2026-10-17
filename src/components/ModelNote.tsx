import { useApp } from "@/store/app";
import { modelNote, type NotePlace } from "@/lib/modelNote";
import type { Tone } from "@/lib/status";

const toneClass: Record<Tone, string> = {
  neutral: "border-border text-text-2",
  success: "border-success/40 text-success",
  warning: "border-warning/40 text-warning",
  danger: "border-danger/40 text-danger",
};

/** One line about whether the user's DLSS 5 model suits their GPU. */
export function ModelNote({ place, className = "" }: { place: NotePlace; className?: string }) {
  const status = useApp((s) => s.modelStatus);
  const note = modelNote(status, place);
  if (!note) return null;
  return (
    <p className={`rounded-md border px-3 py-2 text-xs ${toneClass[note.tone]} ${className}`} role="note">
      {note.text}
    </p>
  );
}
