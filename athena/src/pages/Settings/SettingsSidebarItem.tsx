import type { ReactNode } from "react";

type Props = {
  label: string;
  icon: ReactNode;
  selected?: boolean;
  onClick: () => void;
  badge?: string | number;
  iconTone?: string;
};

export function SettingsSidebarItem({ label, icon, selected, onClick, badge, iconTone }: Props) {
  return (
    <button
      type="button"
      className={["settings-sidebar-item", selected ? "settings-sidebar-item--selected" : ""]
        .filter(Boolean)
        .join(" ")}
      onClick={onClick}
      aria-current={selected ? "page" : undefined}
    >
      <span
        className="settings-sidebar-item__squircle"
        style={iconTone ? { background: iconTone } : undefined}
        aria-hidden
      >
        {icon}
      </span>
      <span className="settings-sidebar-item__label">{label}</span>
      {badge != null && badge !== "" ? (
        <span className="settings-sidebar-item__badge">{badge}</span>
      ) : null}
    </button>
  );
}
