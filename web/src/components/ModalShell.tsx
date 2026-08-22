import { type ReactNode } from "react";
import { Dialog, type DialogProps, Modal, ModalOverlay } from "react-aria-components";

export default function ModalShell({
  open,
  onOpenChange,
  children,
  maxWidth = "28rem",
  dismissable = true,
  label,
  variant = "dialog",
  ...dialogProps
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: ReactNode;
  maxWidth?: string;
  dismissable?: boolean;
  label: string;
  /** Centered dialog (default) or right-edge drawer (Sources panel). */
  variant?: "dialog" | "drawer";
} & Omit<DialogProps, "children" | "className">) {
  if (variant === "drawer") {
    return (
      <ModalOverlay
        isOpen={open}
        isDismissable={dismissable}
        onOpenChange={onOpenChange}
        className="fixed inset-0 z-40 bg-[rgba(0,0,0,0.2)]"
      >
        <Modal className="fixed top-0 bottom-0 right-0 z-50 w-[320px] overflow-auto bg-panel p-6 shadow-[-2px_0_8px_rgba(0,0,0,0.1)] outline-none">
          <Dialog aria-label={label} className="outline-none" {...dialogProps}>
            {children}
          </Dialog>
        </Modal>
      </ModalOverlay>
    );
  }

  return (
    <ModalOverlay
      isOpen={open}
      isDismissable={dismissable}
      onOpenChange={onOpenChange}
      className="fixed inset-0 z-[200] flex items-center justify-center bg-scrim p-4"
    >
      <Modal
        className="relative w-full rounded-lg border border-border bg-panel p-5 shadow-[0_16px_48px_rgba(0,0,0,0.25)] outline-none"
        style={{ maxWidth }}
      >
        <Dialog aria-label={label} className="outline-none" {...dialogProps}>
          {children}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
