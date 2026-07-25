import type { ReactNode } from "react";

type Props = {
  label?: string;
  children: ReactNode;
  className?: string;
  focusId?: string;
};

export function SettingsGroup({ label, children, className, focusId }: Props) {
  return (
    <section className={["settings-group-block", className].filter(Boolean).join(" ")} data-settings-focus={focusId}>
      {label ? <h3 className="settings-group-label">{label}</h3> : null}
      <div className="settings-group">{children}</div>
    </section>
  );
}
