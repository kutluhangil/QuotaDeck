/**
 * An empty panel is an instruction, not an apology. Each state says what is missing and
 * what the user can do about it.
 *
 * The second action is deliberately quieter than the first. Where both exist, one of them is
 * the real fix and the other is a way out of a dead end — offering them at the same weight
 * would leave the user guessing which is which.
 */
export function EmptyState({
  title,
  body,
  action,
  onAction,
  secondaryAction,
  onSecondaryAction,
}: {
  title: string;
  body: string;
  action?: string;
  onAction?: () => void;
  secondaryAction?: string;
  onSecondaryAction?: () => void;
}) {
  return (
    <div className="empty">
      <h2 className="type-label empty__title">{title}</h2>
      <p className="type-body empty__body">{body}</p>
      {action && onAction && (
        <button type="button" className="type-body empty__action" onClick={onAction}>
          {action}
        </button>
      )}
      {secondaryAction && onSecondaryAction && (
        <button
          type="button"
          className="type-caption empty__action empty__action--quiet"
          onClick={onSecondaryAction}
        >
          {secondaryAction}
        </button>
      )}
    </div>
  );
}
