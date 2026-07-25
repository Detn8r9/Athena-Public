import type { ButtonHTMLAttributes } from "react";

type Props = {
  checked: boolean;
  onCheckedChange: (next: boolean) => void;
  disabled?: boolean;
  id?: string;
  "aria-label"?: string;
} & Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange" | "role" | "type">;

export function PillToggle({
  checked,
  onCheckedChange,
  disabled,
  id,
  "aria-label": ariaLabel,
  className,
  ...rest
}: Props) {
  return (
    <button
      {...rest}
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      className={["pill-toggle", checked ? "pill-toggle--on" : "", className].filter(Boolean).join(" ")}
      onClick={() => {
        if (disabled) return;
        onCheckedChange(!checked);
      }}
    >
      <span className="pill-toggle__knob" aria-hidden />
    </button>
  );
}
