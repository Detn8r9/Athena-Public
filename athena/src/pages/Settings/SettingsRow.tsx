import type { ReactNode } from "react";

type Props = {
  title: string;
  subtitle?: ReactNode;
  icon?: ReactNode;
  trailing?: ReactNode;
  focusId?: string;
  spotlight?: boolean;
  onClick?: () => void;
  chevron?: boolean;
  disabled?: boolean;
  className?: string;
};

export function SettingsRow({
  title,
  subtitle,
  icon,
  trailing,
  focusId,
  spotlight,
  onClick,
  chevron,
  disabled,
  className,
}: Props) {
  const interactive = Boolean(onClick) && !disabled;
  const classNames = [
    "settings-row",
    interactive ? "settings-row--interactive" : "",
    spotlight ? "settings-row--spotlight" : "",
    disabled ? "settings-row--disabled" : "",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  const body = (
    <>
      {icon ? (
        <span className="settings-row__icon" aria-hidden>
          {icon}
        </span>
      ) : null}
      <span className="settings-row__text">
        <span className="settings-row__title">{title}</span>
        {subtitle ? <span className="settings-row__subtitle">{subtitle}</span> : null}
      </span>
      {trailing ? (
        <span className="settings-row__trailing" onClick={(e) => e.stopPropagation()}>
          {trailing}
        </span>
      ) : null}
      {chevron ? (
        <span className="settings-row__chevron" aria-hidden>
          ›
        </span>
      ) : null}
    </>
  );

  if (interactive) {
    return (
      <button
        type="button"
        className={classNames}
        data-settings-focus={focusId}
        onClick={onClick}
        disabled={disabled}
      >
        {body}
      </button>
    );
  }

  return (
    <div className={classNames} data-settings-focus={focusId}>
      {body}
    </div>
  );
}
