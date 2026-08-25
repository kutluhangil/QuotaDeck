import type { Catalogue } from "../i18n";
import { useStrings } from "../store";
import type { VisibleProviderHealth } from "../providerHealth";

export function ProviderHealthNotice({ health }: { health: VisibleProviderHealth | null }) {
  const strings: Catalogue = useStrings();
  if (health === null) return null;

  return (
    <p
      className="type-caption card__health"
      data-health={health.state}
      role={health.state === "error" ? "alert" : "status"}
    >
      {strings.health[health.state](health.lastError)}
    </p>
  );
}
