interface RefreshCompletionState {
  refreshGeneration: number;
  refreshError: string | null;
}

export function completeRefresh(
  pendingRequest: number | null,
  state: RefreshCompletionState,
): { pendingRequest: number | null; error: string | null } {
  if (pendingRequest === null) {
    return { pendingRequest: null, error: state.refreshError };
  }
  if (state.refreshGeneration < pendingRequest) {
    return { pendingRequest, error: null };
  }
  return { pendingRequest: null, error: state.refreshError };
}
