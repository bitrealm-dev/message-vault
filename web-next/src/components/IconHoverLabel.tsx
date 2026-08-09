"use client";

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

const HOVER_LABEL_DELAY_MS = 1000;

function FloatingTooltip({
  label,
  placement,
  anchorRect,
}: {
  label: ReactNode;
  placement: "right" | "bottom";
  anchorRect: DOMRect;
}) {
  const style: CSSProperties =
    placement === "right"
      ? {
          position: "fixed",
          top: anchorRect.top + anchorRect.height / 2,
          left: anchorRect.right + 6,
          transform: "translateY(-50%)",
          zIndex: 10000,
        }
      : {
          position: "fixed",
          top: anchorRect.bottom + 6,
          left: anchorRect.left + anchorRect.width / 2,
          transform: "translateX(-50%)",
          zIndex: 10000,
        };

  const caretClass =
    placement === "right"
      ? "before:absolute before:top-1/2 before:right-full before:-translate-y-1/2 before:border-[5px] before:border-transparent before:border-r-panel before:content-['']"
      : "before:absolute before:bottom-full before:left-1/2 before:-translate-x-1/2 before:border-[5px] before:border-transparent before:border-b-panel before:content-['']";

  return (
    <span
      role="tooltip"
      style={style}
      className={`pointer-events-none whitespace-nowrap rounded-md bg-panel px-2.5 py-1 text-[13px] font-medium text-text shadow-lg ${caretClass}`}
    >
      {label}
    </span>
  );
}

/** Hover label portaled to document.body so it clears panel overflow and z-index. */
export function IconHoverTarget({
  children,
  label,
  placement = "bottom",
  className,
  hidden = false,
}: {
  children: ReactNode;
  label: ReactNode;
  placement?: "right" | "bottom";
  className?: string;
  /** Hide while a menu/popover is open on the control. */
  hidden?: boolean;
}) {
  const anchorRef = useRef<HTMLDivElement>(null);
  const suppressShowRef = useRef(false);
  const showTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [anchorRect, setAnchorRect] = useState<DOMRect | null>(null);

  const clearShowTimer = useCallback(() => {
    if (showTimerRef.current) {
      clearTimeout(showTimerRef.current);
      showTimerRef.current = null;
    }
  }, []);

  const hide = useCallback(() => {
    clearShowTimer();
    suppressShowRef.current = false;
    setAnchorRect(null);
  }, [clearShowTimer]);

  const dismiss = useCallback(() => {
    clearShowTimer();
    suppressShowRef.current = true;
    setAnchorRect(null);
  }, [clearShowTimer]);

  const scheduleShow = useCallback(() => {
    if (suppressShowRef.current || hidden) return;
    const el = anchorRef.current;
    if (!el) return;
    clearShowTimer();
    showTimerRef.current = setTimeout(() => {
      showTimerRef.current = null;
      if (suppressShowRef.current || hidden) return;
      setAnchorRect(el.getBoundingClientRect());
    }, HOVER_LABEL_DELAY_MS);
  }, [hidden, clearShowTimer]);

  useEffect(() => {
    if (hidden) dismiss();
  }, [hidden, dismiss]);

  useEffect(() => () => clearShowTimer(), [clearShowTimer]);

  return (
    <>
      <div
        ref={anchorRef}
        className={className}
        onMouseEnter={scheduleShow}
        onMouseLeave={hide}
        onPointerDownCapture={dismiss}
        onFocus={scheduleShow}
        onBlur={hide}
      >
        {children}
      </div>
      {anchorRect && !hidden &&
        createPortal(
          <FloatingTooltip
            label={label}
            placement={placement}
            anchorRect={anchorRect}
          />,
          document.body,
        )}
    </>
  );
}
