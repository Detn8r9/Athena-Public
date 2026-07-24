import { FormEvent, useState } from "react";

type Props = {
  onSubmit: (cmd: string) => void | Promise<void>;
  onOpenApps: () => void;
  onOpenSettings: () => void;
};

export function CommandBar({ onSubmit, onOpenApps, onOpenSettings }: Props) {
  const [value, setValue] = useState("");

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const v = value;
    setValue("");
    await onSubmit(v);
  };

  return (
    <form className="command-bar" onSubmit={submit}>
      <input
        value={value}
        onChange={(e) => setValue(e.target.value)}
        placeholder="Command bar — try “apps”, “settings”, or “install jellyfin”"
        aria-label="Command bar"
      />
      <button type="button" className="btn ghost" onClick={onOpenSettings}>
        Layout
      </button>
      <button type="button" className="btn primary" onClick={onOpenApps}>
        More Apps
      </button>
    </form>
  );
}
