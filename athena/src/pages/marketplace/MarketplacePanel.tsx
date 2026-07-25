import { Store } from "lucide-react";
import { SettingsGroup } from "../Settings/SettingsGroup";
import { SettingsRow } from "../Settings/SettingsRow";

type Props = {
  onOpenMarketplace?: () => void;
};

/** Settings shortcut — full Discover UI lives on the Marketplace home view. */
export function MarketplacePanel({ onOpenMarketplace }: Props) {
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
      <p className="text-muted small" style={{ margin: "0.5rem 0 0" }}>
        Installs still require Docker on this machine. Offline Connectivity blocks new image pulls.
      </p>
    </SettingsGroup>
  );
}
