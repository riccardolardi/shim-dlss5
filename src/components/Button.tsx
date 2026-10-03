import type { ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "danger";

const styles: Record<Variant, string> = {
  primary: "bg-accent text-accent-contrast hover:brightness-95",
  secondary: "bg-surface-2 text-text border border-border hover:bg-border",
  danger: "bg-danger text-danger-contrast hover:brightness-95",
};

export function Button({
  variant = "secondary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      type="button"
      {...props}
      className={[
        "inline-flex h-9 items-center justify-center gap-2 rounded-md px-3.5 text-sm font-medium transition",
        "disabled:cursor-not-allowed disabled:opacity-50",
        styles[variant],
        className,
      ].join(" ")}
    />
  );
}
