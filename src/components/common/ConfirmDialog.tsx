interface ConfirmDialogProps {
  isOpen: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  isOpen,
  title,
  message,
  confirmLabel = "Yes",
  cancelLabel = "No",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  if (!isOpen) return null;

  return (
    <>
      <div
        className="fixed inset-0 bg-black/40 z-[60]"
        onClick={onCancel}
        aria-hidden
      />
      <div
        className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 z-[61] w-full max-w-md rounded-xl border border-border bg-app-card shadow-2xl"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        aria-describedby="confirm-dialog-message"
      >
        <div className="p-6">
          <h2
            id="confirm-dialog-title"
            className="text-lg font-semibold text-text-primary mb-2"
          >
            {title}
          </h2>
          <p
            id="confirm-dialog-message"
            className="text-text-primary mb-6"
          >
            {message}
          </p>
          <div className="flex justify-end gap-3">
            <button
              type="button"
              onClick={onCancel}
              className="h-10 px-4 rounded-md bg-app-card border border-border text-text-primary hover:bg-app-card-hover transition-colors"
            >
              {cancelLabel}
            </button>
            <button
              type="button"
              onClick={onConfirm}
              className="h-10 px-4 rounded-md bg-accent-blue text-white font-medium hover:bg-accent-blue/90 transition-colors"
            >
              {confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
