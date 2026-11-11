import { useCallback, useEffect, useState } from "react";
import { Store } from "lucide-react";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { SettingsGroup } from "../Settings/SettingsGroup";
import { SettingsRow } from "../Settings/SettingsRow";
import { PillToggle } from "../Settings/PillToggle";

type Props = {
  onOpenMarketplace?: () => void;
};

/** Settings shortcut — full Discover UI lives on the Marketplace home view. */
export function MarketplacePanel({ onOpenMarketplace }: Props) {
  const [autoUpdate, setAutoUpdate] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const o = await invoke<{ autoUpdate: boolean }>("marketplace_overview");
      setAutoUpdate(Boolean(o.autoUpdate));
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function onToggleAutoUpdate(enabled: boolean) {
    setBusy(true);
    setErr(null);
    try {
      const next = await invoke<boolean>("marketplace_set_auto_update", { enabled });
      setAutoUpdate(next);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <SettingsGroup label="Marketplace">
      <SettingsRow
        title="Open Marketplace"
        subtitle="Docker apps (Jellyfin, Portainer) and voice modules. Discover lives on the home Marketplace screen."
        trailing={
          <button type="button" className="marketplace-get-btn" onClick={() => onOpenMarketplace?.()}>
            <Store size={16} strokeWidth={2} aria-hidden style={{ marginRight: "0.35rem", verticalAlign: "middle" }} />
            Open
          </button>
        }
        onClick={() => onOpenMarketplace?.()}
        chevron
      />
      <SettingsRow
        focusId="marketplace-auto-update"
        title="Auto-update modules"
        subtitle="When catalog settings change, recreate installed containers automatically (keeps model caches)"
        trailing={
          <PillToggle
            checked={autoUpdate}
            disabled={busy}
            onCheckedChange={(v) => void onToggleAutoUpdate(v)}
            aria-label="Auto-update Marketplace modules"
          />
        }
      />
      <p className="text-muted small" style={{ margin: "0.5rem 0 0" }}>
        Installs still require Docker on this machine. Offline Connectivity blocks new image pulls.
        Stop/Start alone does not apply catalog changes — use Update or enable auto-update.
      </p>
      {err ? (
        <p className="text-muted small" role="alert" style={{ margin: "0.35rem 0 0", color: "var(--danger, #c44)" }}>
          {err}
        </p>
      ) : null}
    </SettingsGroup>
  );
}
