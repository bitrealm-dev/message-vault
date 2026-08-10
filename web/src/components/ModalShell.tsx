import { type ReactNode } from "react";
import { Dialog, Modal, ModalOverlay, type DialogProps } from "react-aria-components";

export default function ModalShell({
  open,
  onOpenChange,
  children,
  maxWidth = "28rem",
  dismissable = true,
  label,
  ...dialogProps
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: ReactNode;
  maxWidth?: string;
  dismissable?: boolean;
  label: string;
} & Omit<DialogProps, "children" | "className">) {
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
