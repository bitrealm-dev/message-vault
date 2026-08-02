"use client";

import { AppShell } from "@/components/AppShell";
import { CountBadge } from "@/components/CountBadge";
import { useDateTimeFormat } from "@/components/useDateTimeFormat";
import type { HomeStats } from "@/lib/types";
import Link from "next/link";

function StatCard({
  href,
  label,
  value,
  detail,
}: {
  href?: string;
  label: string;
  value: number;
  detail: string;
}) {
  const content = (
    <>
      <div className="text-[12px] text-muted">{label}</div>
      <div className="mt-1 text-2xl font-semibold tabular-nums">
        {value.toLocaleString()}
      </div>
      <div className="mt-2 text-[12px] text-muted">{detail}</div>
    </>
  );
  const className =
    "rounded-lg border border-border bg-panel px-4 py-4 transition";
  return href ? (
    <Link href={href} className={`${className} hover:border-accent/50`}>
      {content}
    </Link>
  ) : (
    <div className={className}>{content}</div>
  );
}

export function HomePageClient({
  labels,
  stats,
}: {
  labels: string[];
  stats: HomeStats;
}) {
  const { formatDate, formatDateRange } = useDateTimeFormat();
  const totalDirectional = stats.sentMessages + stats.receivedMessages;
  const sentPercent =
    totalDirectional === 0
      ? 0
      : Math.round((stats.sentMessages / totalDirectional) * 100);

  return (
    <AppShell active="/" labels={labels}>
      <main className="h-full min-h-0 min-w-0 overflow-y-auto bg-bg px-8 py-10">
        <div className="max-w-5xl">
          <h1 className="text-2xl font-semibold tracking-tight">
            Message Vault
          </h1>
          <p className="mt-2 max-w-2xl text-[14px] text-muted">
            An overview of your conversations and the people you keep in touch
            with.
          </p>

          <section className="mt-8">
            <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
              Overview
            </h2>
            <div className="mt-3 grid grid-cols-2 gap-4 lg:grid-cols-4">
              <StatCard
                href="/all"
                label="Messages"
                value={stats.messages}
                detail="Across direct and group conversations"
              />
              <StatCard
                href="/all"
                label="Contacts"
                value={stats.contacts}
                detail="Contacts in your vault"
              />
              <StatCard
                href="/group-messages"
                label="Group chats"
                value={stats.groupChats}
                detail="Conversations with multiple people"
              />
              <StatCard
                label="Attachments"
                value={stats.attachments}
                detail="Photos, files, and other media"
              />
            </div>
          </section>

          <div className="mt-8 grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(16rem,1fr)]">
            <section>
              <div className="flex items-center justify-between gap-4">
                <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
                  Recent contacts
                </h2>
                <Link
                  href="/all"
                  className="text-[12px] text-accent hover:text-text"
                >
                  View all
                </Link>
              </div>

              <div className="mt-3 overflow-hidden rounded-lg border border-border bg-panel">
                {stats.recentContacts.length > 0 ? (
                  stats.recentContacts.map((contact, index) => (
                    <Link
                      key={contact.id}
                      href={`/all?c=${contact.id}`}
                      className={`flex items-center justify-between gap-4 px-4 py-3 transition-colors hover:bg-hover ${
                        index > 0 ? "border-t border-border" : ""
                      }`}
                    >
                      <div className="min-w-0">
                        <div className="truncate text-[14px] font-medium text-text">
                          {contact.displayName}
                        </div>
                        <div className="mt-0.5 text-[12px] text-muted">
                          Last message {formatDate(contact.dateEnd)}
                        </div>
                      </div>
                      <div className="flex shrink-0 items-center gap-2 text-[12px] text-muted">
                        {contact.groupChatCount > 0 ? (
                          <span>
                            {contact.groupChatCount.toLocaleString()} groups
                          </span>
                        ) : null}
                        <CountBadge
                          count={contact.messageCount}
                          title="Direct messages"
                        />
                      </div>
                    </Link>
                  ))
                ) : (
                  <p className="px-4 py-8 text-center text-[13px] text-muted">
                    Import messages to see recent contacts.
                  </p>
                )}
              </div>
            </section>

            <section>
              <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
                Vault history
              </h2>
              <div className="mt-3 rounded-lg border border-border bg-panel p-4">
                <div className="text-[13px] text-muted">Message history</div>
                <div className="mt-1 text-[14px] font-medium text-text">
                  {stats.dateStart && stats.dateEnd
                    ? formatDateRange(stats.dateStart, stats.dateEnd)
                    : "No messages yet"}
                </div>

                <div className="mt-5 flex items-center justify-between text-[12px]">
                  <span className="text-muted">Sent</span>
                  <span className="tabular-nums text-text">
                    {stats.sentMessages.toLocaleString()}
                  </span>
                </div>
                <div className="mt-2 h-2 overflow-hidden rounded-full bg-elevated">
                  <div
                    className="h-full rounded-full bg-accent"
                    style={{ width: `${sentPercent}%` }}
                  />
                </div>
                <div className="mt-2 flex items-center justify-between text-[12px]">
                  <span className="text-muted">Received</span>
                  <span className="tabular-nums text-text">
                    {stats.receivedMessages.toLocaleString()}
                  </span>
                </div>

                <div className="mt-5 space-y-2 border-t border-border pt-4 text-[12px]">
                  <div className="flex items-center justify-between">
                    <span className="text-muted">Import sources</span>
                    <span className="tabular-nums text-text">
                      {stats.sources.toLocaleString()}
                    </span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted">Duplicate copies</span>
                    <span className="tabular-nums text-text">
                      {stats.messageDuplicates.toLocaleString()}
                    </span>
                  </div>
                </div>
              </div>
            </section>
          </div>

          <section className="mt-8">
            <h2 className="text-[12px] font-semibold tracking-wider text-muted uppercase">
              Explore
            </h2>
            <div className="mt-3 grid grid-cols-2 gap-4 sm:max-w-2xl">
              <StatCard
                href="/all"
                label="All contacts"
                value={stats.all}
                detail="Browse everyone"
              />
              <StatCard
                href="/no-messages"
                label="No messages"
                value={stats.noMessages}
                detail="Contacts without conversations"
              />
            </div>
          </section>
        </div>
      </main>
    </AppShell>
  );
}
