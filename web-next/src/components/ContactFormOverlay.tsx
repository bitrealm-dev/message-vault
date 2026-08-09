"use client";

import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";

export type ContactFormAnchor = {
  top: number;
  left: number;
  width: number;
  height: number;
  bottom: number;
  right: number;
};

export function contactFormAnchorFromRect(rect: DOMRect): ContactFormAnchor {
  return {
    top: rect.top,
    left: rect.left,
    width: rect.width,
    height: rect.height,
    bottom: rect.bottom,
    right: rect.right,
  };
}

const MARGIN = 8;
const GAP = 6;

function clampPosition(
  anchor: ContactFormAnchor,
  panelWidth: number,
  panelHeight: number,
): { top: number; left: number } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const width = Math.min(panelWidth, vw - MARGIN * 2);

  let left = anchor.left + anchor.width / 2 - width / 2;
  left = Math.max(MARGIN, Math.min(left, vw - width - MARGIN));

  const spaceBelow = vh - anchor.bottom - GAP - MARGIN;
  const spaceAbove = anchor.top - GAP - MARGIN;
  const preferBelow =
    spaceBelow >= Math.min(panelHeight, 280) || spaceBelow >= spaceAbove;

  if (preferBelow) {
    const top = Math.min(
      anchor.bottom + GAP,
      vh - MARGIN - Math.min(panelHeight, spaceBelow),
    );
    return { top: Math.max(MARGIN, top), left };
  }

  const top = Math.max(MARGIN, anchor.top - GAP - panelHeight);
  return { top, left };
}

/** Centered modal, or popover anchored to a name chip. */
export function ContactFormOverlay({
  anchor,
  titleId,
  title,
  busy,
  onDismiss,
  children,
  footer,
}: {
  anchor: ContactFormAnchor | null;
  titleId: string;
  title: string;
  busy: boolean;
  onDismiss: () => void;
  children: ReactNode;
  footer: ReactNode;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{
    top: number;
    left: number;
  } | null>(null);
  const [entered, setEntered] = useState(false);

  useLayoutEffect(() => {
    if (!anchor) {
      setPos(null);
      return;
    }
    const el = panelRef.current;
    if (!el) return;

    const update = () => {
      setPos(clampPosition(anchor, el.offsetWidth, el.offsetHeight));
    };
    update();

    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [anchor]);

  useEffect(() => {
    if (!anchor) {
      setEntered(false);
      return;
    }
    const id = requestAnimationFrame(() => setEntered(true));
    return () => cancelAnimationFrame(id);
  }, [anchor]);

  useEffect(() => {
    if (!anchor) return;
    const onResize = () => {
      const el = panelRef.current;
      if (!el) return;
      setPos(clampPosition(anchor, el.offsetWidth, el.offsetHeight));
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [anchor]);

  if (anchor) {
    return (
      <div
        className="fixed inset-0 z-[200]"
        role="presentation"
        onClick={() => {
          if (!busy) onDismiss();
        }}
      >
        <div
          ref={panelRef}
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          className={`fixed w-[min(32rem,calc(100vw-1rem))] rounded-xl border border-border bg-popover p-5 shadow-[0_16px_48px_rgba(0,0,0,0.5)] transition-opacity duration-150 ease-out ${
            entered ? "opacity-100" : "opacity-0"
          }`}
          style={
            pos
              ? { top: pos.top, left: pos.left }
              : {
                  top: anchor.bottom + GAP,
                  left: Math.max(
                    MARGIN,
                    Math.min(
                      anchor.left + anchor.width / 2 - 256,
                      window.innerWidth - 512 - MARGIN,
                    ),
                  ),
                  visibility: "hidden",
                }
          }
          onClick={(e) => e.stopPropagation()}
        >
          <h2 id={titleId} className="text-[16px] font-semibold text-text">
            {title}
          </h2>
          <div className="mt-4">{children}</div>
          <div className="mt-4 flex justify-end gap-2">{footer}</div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-scrim px-4"
      role="presentation"
      onClick={() => {
        if (!busy) onDismiss();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="w-full max-w-lg rounded-xl border border-border bg-popover p-5 shadow-[0_16px_48px_rgba(0,0,0,0.5)]"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id={titleId} className="text-[16px] font-semibold text-text">
          {title}
        </h2>
        <div className="mt-4">{children}</div>
        <div className="mt-4 flex justify-end gap-2">{footer}</div>
      </div>
    </div>
  );
}
