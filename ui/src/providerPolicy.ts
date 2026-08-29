import type {
  ProviderDescriptor,
  ProviderInstanceId,
  ProviderPolicyOutcome,
  Settings,
} from "./types";

export function providerPolicySettings(
  settings: Settings,
  disabledProviders: ProviderInstanceId[],
  providerOrder: ProviderInstanceId[],
): Settings {
  return { ...settings, disabledProviders: [...disabledProviders], providerOrder: [...providerOrder] };
}

export async function applyProviderPolicy(
  previous: Settings,
  optimistic: Settings,
  persist: (settings: Settings) => Promise<ProviderPolicyOutcome>,
): Promise<{ settings: Settings; error: string | null; persisted: boolean }> {
  try {
    const outcome = await persist(optimistic);
    return { settings: outcome.settings, error: outcome.warning, persisted: true };
  } catch (error) {
    return { settings: previous, error: String(error), persisted: false };
  }
}

export function catalogueForPolicy(
  catalogue: ProviderDescriptor[],
  settings: Settings,
): ProviderDescriptor[] {
  const byId = new Map(catalogue.map((entry) => [entry.id, entry]));
  return settings.providerOrder.flatMap((id) => {
    const entry = byId.get(id);
    return entry === undefined
      ? []
      : [{ ...entry, enabled: !settings.disabledProviders.includes(id) }];
  });
}

export function focusDirectionAfterMove(
  index: number,
  length: number,
  requested: -1 | 1,
): -1 | 1 | null {
  const canUseRequested = requested === -1 ? index > 0 : index < length - 1;
  if (canUseRequested) return requested;
  const opposite = requested === -1 ? 1 : -1;
  const canUseOpposite = opposite === -1 ? index > 0 : index < length - 1;
  return canUseOpposite ? opposite : null;
}
