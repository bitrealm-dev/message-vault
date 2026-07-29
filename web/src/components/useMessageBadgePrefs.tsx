"use client";

import {
  DEFAULT_SHOW_CONTACT_DATE_RANGE,
  DEFAULT_SHOW_CONTACT_INITIALS,
  DEFAULT_SHOW_GROUP_MESSAGE_BADGE,
  DEFAULT_SHOW_MESSAGE_BADGE,
  isBadgeVisibility,
  readBadgeVisibility,
  SHOW_CONTACT_DATE_RANGE_KEY,
  SHOW_CONTACT_INITIALS_KEY,
  SHOW_GROUP_MESSAGE_BADGE_KEY,
  SHOW_MESSAGE_BADGE_KEY,
  type BadgeVisibility,
} from "@/lib/messageBadgePrefs";
import {
  fetchServerPrefs,
  pushServerPrefs,
  reconcilePrefs,
} from "@/lib/prefsClient";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

const BADGE_PREF_KEYS = [
  SHOW_MESSAGE_BADGE_KEY,
  SHOW_GROUP_MESSAGE_BADGE_KEY,
  SHOW_CONTACT_INITIALS_KEY,
  SHOW_CONTACT_DATE_RANGE_KEY,
] as const;

export type MessageBadgePrefs = {
  showMessageBadge: boolean;
  showGroupMessageBadge: boolean;
  showContactInitials: boolean;
  showContactDateRange: boolean;
  setShowMessageBadge: (on: boolean) => void;
  setShowGroupMessageBadge: (on: boolean) => void;
  setShowContactInitials: (on: boolean) => void;
  setShowContactDateRange: (on: boolean) => void;
};

const MessageBadgePrefsContext = createContext<MessageBadgePrefs | null>(null);

function toBool(v: BadgeVisibility): boolean {
  return v === "on";
}

function fromBool(on: boolean): BadgeVisibility {
  return on ? "on" : "off";
}

function visibilityFromReconcile(
  raw: string | null | undefined,
  fallback: BadgeVisibility,
): BadgeVisibility {
  return isBadgeVisibility(raw) ? raw : fallback;
}

export function MessageBadgePrefsProvider({
  children,
}: {
  children: ReactNode;
}) {
  const [showMessageBadge, setShowMessageBadgeState] = useState(false);
  const [showGroupMessageBadge, setShowGroupMessageBadgeState] = useState(false);
  const [showContactInitials, setShowContactInitialsState] = useState(true);
  const [showContactDateRange, setShowContactDateRangeState] = useState(false);

  useEffect(() => {
    setShowMessageBadgeState(
      toBool(
        readBadgeVisibility(SHOW_MESSAGE_BADGE_KEY, DEFAULT_SHOW_MESSAGE_BADGE),
      ),
    );
    setShowGroupMessageBadgeState(
      toBool(
        readBadgeVisibility(
          SHOW_GROUP_MESSAGE_BADGE_KEY,
          DEFAULT_SHOW_GROUP_MESSAGE_BADGE,
        ),
      ),
    );
    setShowContactInitialsState(
      toBool(
        readBadgeVisibility(
          SHOW_CONTACT_INITIALS_KEY,
          DEFAULT_SHOW_CONTACT_INITIALS,
        ),
      ),
    );
    setShowContactDateRangeState(
      toBool(
        readBadgeVisibility(
          SHOW_CONTACT_DATE_RANGE_KEY,
          DEFAULT_SHOW_CONTACT_DATE_RANGE,
        ),
      ),
    );

    let cancelled = false;
    void fetchServerPrefs().then((serverPrefs) => {
      if (cancelled || !serverPrefs) return;
      const { values, toPush } = reconcilePrefs(serverPrefs, BADGE_PREF_KEYS);
      setShowMessageBadgeState(
        toBool(
          visibilityFromReconcile(
            values[SHOW_MESSAGE_BADGE_KEY],
            DEFAULT_SHOW_MESSAGE_BADGE,
          ),
        ),
      );
      setShowGroupMessageBadgeState(
        toBool(
          visibilityFromReconcile(
            values[SHOW_GROUP_MESSAGE_BADGE_KEY],
            DEFAULT_SHOW_GROUP_MESSAGE_BADGE,
          ),
        ),
      );
      setShowContactInitialsState(
        toBool(
          visibilityFromReconcile(
            values[SHOW_CONTACT_INITIALS_KEY],
            DEFAULT_SHOW_CONTACT_INITIALS,
          ),
        ),
      );
      setShowContactDateRangeState(
        toBool(
          visibilityFromReconcile(
            values[SHOW_CONTACT_DATE_RANGE_KEY],
            DEFAULT_SHOW_CONTACT_DATE_RANGE,
          ),
        ),
      );
      if (Object.keys(toPush).length > 0) pushServerPrefs(toPush);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const setShowMessageBadge = useCallback((on: boolean) => {
    setShowMessageBadgeState(on);
    const v = fromBool(on);
    window.localStorage.setItem(SHOW_MESSAGE_BADGE_KEY, v);
    pushServerPrefs({ [SHOW_MESSAGE_BADGE_KEY]: v });
  }, []);

  const setShowGroupMessageBadge = useCallback((on: boolean) => {
    setShowGroupMessageBadgeState(on);
    const v = fromBool(on);
    window.localStorage.setItem(SHOW_GROUP_MESSAGE_BADGE_KEY, v);
    pushServerPrefs({ [SHOW_GROUP_MESSAGE_BADGE_KEY]: v });
  }, []);

  const setShowContactInitials = useCallback((on: boolean) => {
    setShowContactInitialsState(on);
    const v = fromBool(on);
    window.localStorage.setItem(SHOW_CONTACT_INITIALS_KEY, v);
    pushServerPrefs({ [SHOW_CONTACT_INITIALS_KEY]: v });
  }, []);

  const setShowContactDateRange = useCallback((on: boolean) => {
    setShowContactDateRangeState(on);
    const v = fromBool(on);
    window.localStorage.setItem(SHOW_CONTACT_DATE_RANGE_KEY, v);
    pushServerPrefs({ [SHOW_CONTACT_DATE_RANGE_KEY]: v });
  }, []);

  const value = useMemo(
    () => ({
      showMessageBadge,
      showGroupMessageBadge,
      showContactInitials,
      showContactDateRange,
      setShowMessageBadge,
      setShowGroupMessageBadge,
      setShowContactInitials,
      setShowContactDateRange,
    }),
    [
      showMessageBadge,
      showGroupMessageBadge,
      showContactInitials,
      showContactDateRange,
      setShowMessageBadge,
      setShowGroupMessageBadge,
      setShowContactInitials,
      setShowContactDateRange,
    ],
  );

  return (
    <MessageBadgePrefsContext.Provider value={value}>
      {children}
    </MessageBadgePrefsContext.Provider>
  );
}

export function useMessageBadgePrefs(): MessageBadgePrefs {
  const ctx = useContext(MessageBadgePrefsContext);
  if (!ctx) {
    throw new Error(
      "useMessageBadgePrefs must be used within MessageBadgePrefsProvider",
    );
  }
  return ctx;
}
