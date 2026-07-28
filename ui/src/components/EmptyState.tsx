/**
 * An empty panel is an instruction, not an apology. Each state says what is missing and
 * what the user can do about it.
 */
export function EmptyState({
  title,
  body,
  action,
  onAction,
}: {
  title: string;
  body: string;
  action?: string;
  onAction?: () => void;
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
    </div>
  );
}
