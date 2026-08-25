/** @vitest-environment jsdom */

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CachedContactDetail } from "../lib/contactDetailCache";
import {
  clearContactDetailCache,
  fetchContactDetail,
  getCachedContactDetail,
} from "../lib/contactDetailCache";
import ContactDrawer from "./ContactDrawer";

const get = vi.fn();

vi.mock("../lib/api", () => ({
  apiClient: {
    get: (...args: unknown[]) => get(...args),
    post: vi.fn(),
  },
}));

function detail(id: string, overrides: Partial<CachedContactDetail> = {}): CachedContactDetail {
  return {
    id,
    name: `Contact ${id}`,
    handles: [
      {
        handle: `+1555000${id}`,
        service: "phone",
        name_alias: null,
        start_date: "2020-01-01T00:00:00Z",
        end_date: "2024-01-01T00:00:00Z",
        individual_conversations: 3,
        group_conversations: 1,
        individual_message_count: 42,
        group_message_count: 7,
      },
    ],
    direct_conversations: 3,
    group_conversations: 1,
    total_messages: 49,
    groups: [`Group-${id}`],
    ...overrides,
  };
}

describe("ContactDrawer", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    clearContactDetailCache();
    get.mockReset();
  });

  it("keeps groups and avoids zero counts on first paint when switching to an uncached contact", async () => {
    const a = detail("a");
    await fetchContactDetail("a", async () => a);

    let resolveB!: (d: CachedContactDetail) => void;
    const pendingB = new Promise<CachedContactDetail>((resolve) => {
      resolveB = resolve;
    });
    get.mockImplementation((path: string) => {
      if (String(path).includes("/a")) return Promise.resolve(a);
      return pendingB;
    });

    const { rerender } = render(
      <ContactDrawer
        variant="docked"
        contactId="a"
        preview={{
          id: "a",
          name: a.name,
          handles: a.handles.map((h) => h.handle),
          groups: a.groups,
        }}
        onClose={() => {}}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: a.name })).toBeTruthy();
      expect(screen.getByText("Group-a")).toBeTruthy();
    });

    rerender(
      <ContactDrawer
        variant="docked"
        contactId="b"
        preview={{
          id: "b",
          name: "Contact b",
          handles: ["+1555000b"],
          groups: ["Family"],
        }}
        onClose={() => {}}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "Contact b" });
    expect(dialog.getAttribute("aria-busy")).toBe("true");
    expect(screen.getByText("Family")).toBeTruthy();
    expect(screen.getByText("+1555000b")).toBeTruthy();

    const table = screen.getByRole("grid", { name: "Contact handles" });
    const dashes = table.textContent?.match(/—/g) ?? [];
    expect(dashes.length).toBeGreaterThanOrEqual(4);

    resolveB(detail("b", { name: "Contact b", groups: ["Family"] }));
    await waitFor(() => {
      expect(dialog.getAttribute("aria-busy")).toBeNull();
      expect(screen.getAllByText("42").length).toBeGreaterThan(0);
    });
  });

  it("shows cached counts and groups on first paint when switching to a cached contact", async () => {
    const a = detail("a");
    const b = detail("b", {
      name: "Cached Bob",
      groups: ["Work"],
      handles: [
        {
          handle: "+15551212",
          service: "phone",
          name_alias: null,
          start_date: "2021-06-01T00:00:00Z",
          end_date: "2025-01-01T00:00:00Z",
          individual_conversations: 5,
          group_conversations: 2,
          individual_message_count: 99,
          group_message_count: 11,
        },
      ],
    });
    await fetchContactDetail("a", async () => a);
    await fetchContactDetail("b", async () => b);
    expect(getCachedContactDetail("b")?.name).toBe("Cached Bob");

    get.mockImplementation((path: string) => {
      if (String(path).includes("/a")) return Promise.resolve(a);
      return Promise.resolve(b);
    });

    const { rerender } = render(
      <ContactDrawer
        variant="docked"
        contactId="a"
        preview={{ id: "a", name: a.name, handles: ["+1555000a"], groups: a.groups }}
        onClose={() => {}}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: a.name })).toBeTruthy();
    });

    rerender(
      <ContactDrawer
        variant="docked"
        contactId="b"
        preview={{
          id: "b",
          name: "Cached Bob",
          handles: ["+15551212"],
          groups: ["Work"],
        }}
        onClose={() => {}}
      />,
    );

    expect(screen.getByRole("heading", { name: "Cached Bob" })).toBeTruthy();
    expect(screen.getByRole("dialog").getAttribute("aria-busy")).toBeNull();
    expect(screen.getByText("Work")).toBeTruthy();
    expect(screen.getAllByText("99").length).toBeGreaterThan(0);
    expect(screen.getAllByText("11").length).toBeGreaterThan(0);
  });

  it("does not claim No groups while loading without preview groups", async () => {
    let resolveDetail!: (d: CachedContactDetail) => void;
    const pending = new Promise<CachedContactDetail>((resolve) => {
      resolveDetail = resolve;
    });
    get.mockImplementation(() => pending);

    render(<ContactDrawer variant="overlay" contactId="z" preview={null} onClose={() => {}} />);

    const dialog = screen.getByRole("dialog", { name: "Loading…" });
    expect(dialog.getAttribute("aria-busy")).toBe("true");
    expect(screen.queryByText("No groups")).toBeNull();
    expect(screen.getByText("…")).toBeTruthy();

    resolveDetail(detail("z", { name: "Zed", groups: ["Work"] }));
    await waitFor(() => {
      expect(screen.getByRole("dialog", { name: "Zed" })).toBeTruthy();
      expect(screen.getByText("Work")).toBeTruthy();
      expect(screen.queryByText("No groups")).toBeNull();
    });
  });

  it("stubs one handle row when preview lists raw and normalized forms of the same identity", async () => {
    let resolveDetail!: (d: CachedContactDetail) => void;
    const pending = new Promise<CachedContactDetail>((resolve) => {
      resolveDetail = resolve;
    });
    get.mockImplementation(() => pending);

    render(
      <ContactDrawer
        variant="docked"
        contactId="b"
        preview={{
          id: "b",
          name: "Contact b",
          handles: ["+1555000b", "1555000b"],
          handleCount: 1,
          groups: ["Family"],
        }}
        onClose={() => {}}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "Contact b" });
    expect(dialog.getAttribute("aria-busy")).toBe("true");
    expect(screen.getByText("+1555000b")).toBeTruthy();
    expect(screen.queryByText("1555000b")).toBeNull();

    const table = screen.getByRole("grid", { name: "Contact handles" });
    // Header + one handle row + summary row.
    expect(table.querySelectorAll('[role="row"]').length).toBe(3);

    resolveDetail(
      detail("b", {
        name: "Contact b",
        groups: ["Family"],
        handles: [
          {
            handle: "+1555000b",
            service: "phone",
            name_alias: null,
            start_date: "2020-01-01T00:00:00Z",
            end_date: "2024-01-01T00:00:00Z",
            individual_conversations: 3,
            group_conversations: 1,
            individual_message_count: 42,
            group_message_count: 7,
          },
        ],
      }),
    );
    await waitFor(() => {
      expect(dialog.getAttribute("aria-busy")).toBeNull();
      expect(table.querySelectorAll('[role="row"]').length).toBe(3);
    });
  });

  it("stubs overlay handles from thread preview while detail is pending", async () => {
    let resolveDetail!: (d: CachedContactDetail) => void;
    const pending = new Promise<CachedContactDetail>((resolve) => {
      resolveDetail = resolve;
    });
    get.mockImplementation(() => pending);

    render(
      <ContactDrawer
        variant="overlay"
        contactId="b"
        preview={{
          id: "b",
          name: "Contact b",
          handles: ["+1555000b"],
          handleCount: 1,
        }}
        onClose={() => {}}
      />,
    );

    expect(screen.getByRole("heading", { name: "Contact b" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Loading…" })).toBeNull();
    expect(screen.getByText("+1555000b")).toBeTruthy();

    const table = screen.getByRole("grid", { name: "Contact handles" });
    expect(table.querySelectorAll('[role="row"]').length).toBe(3);

    resolveDetail(
      detail("b", {
        name: "Contact b",
        handles: [
          {
            handle: "+1555000b",
            service: "phone",
            name_alias: null,
            start_date: "2020-01-01T00:00:00Z",
            end_date: "2024-01-01T00:00:00Z",
            individual_conversations: 3,
            group_conversations: 1,
            individual_message_count: 42,
            group_message_count: 7,
          },
        ],
      }),
    );
    await waitFor(() => {
      expect(table.querySelectorAll('[role="row"]').length).toBe(3);
    });
  });

  it("stubs one overlay identity row when thread preview has no handle strings", async () => {
    let resolveDetail!: (d: CachedContactDetail) => void;
    const pending = new Promise<CachedContactDetail>((resolve) => {
      resolveDetail = resolve;
    });
    get.mockImplementation(() => pending);

    render(
      <ContactDrawer
        variant="overlay"
        contactId="b"
        preview={{
          id: "b",
          name: "Mom",
          handles: [],
          handleCount: 1,
        }}
        onClose={() => {}}
      />,
    );

    expect(screen.getByRole("heading", { name: "Mom" })).toBeTruthy();
    expect(screen.queryByText("Loading…")).toBeNull();

    const table = screen.getByRole("grid", { name: "Contact handles" });
    expect(table.querySelectorAll('[role="row"]').length).toBe(3);
    expect(table.textContent).toContain("…");

    resolveDetail(detail("b", { name: "Ada Lovelace" }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Ada Lovelace" })).toBeTruthy();
    });
  });

  it("stubs two identities when preview lists raw then normalized for each", async () => {
    let resolveDetail!: (d: CachedContactDetail) => void;
    const pending = new Promise<CachedContactDetail>((resolve) => {
      resolveDetail = resolve;
    });
    get.mockImplementation(() => pending);

    render(
      <ContactDrawer
        variant="docked"
        contactId="b"
        preview={{
          id: "b",
          name: "Contact b",
          handles: ["+15550001", "15550001", "+15550002", "15550002"],
          handleCount: 2,
          groups: ["Family"],
        }}
        onClose={() => {}}
      />,
    );

    expect(screen.getByText("+15550001")).toBeTruthy();
    expect(screen.getByText("+15550002")).toBeTruthy();
    expect(screen.queryByText("15550001")).toBeNull();
    expect(screen.queryByText("15550002")).toBeNull();
    const table = screen.getByRole("grid", { name: "Contact handles" });
    expect(table.querySelectorAll('[role="row"]').length).toBe(4);

    resolveDetail(
      detail("b", {
        name: "Contact b",
        groups: ["Family"],
        handles: [
          {
            handle: "+15550001",
            service: "phone",
            name_alias: null,
            start_date: "2020-01-01T00:00:00Z",
            end_date: "2024-01-01T00:00:00Z",
            individual_conversations: 1,
            group_conversations: 0,
            individual_message_count: 4,
            group_message_count: 0,
          },
          {
            handle: "+15550002",
            service: "phone",
            name_alias: null,
            start_date: "2020-01-01T00:00:00Z",
            end_date: "2024-01-01T00:00:00Z",
            individual_conversations: 2,
            group_conversations: 0,
            individual_message_count: 8,
            group_message_count: 0,
          },
        ],
      }),
    );
    await waitFor(() => {
      expect(table.querySelectorAll('[role="row"]').length).toBe(4);
    });
  });

  it("keeps the edit-name control mounted and disabled while detail is loading", async () => {
    let resolveDetail!: (d: CachedContactDetail) => void;
    const pending = new Promise<CachedContactDetail>((resolve) => {
      resolveDetail = resolve;
    });
    get.mockImplementation(() => pending);

    render(
      <ContactDrawer
        variant="docked"
        contactId="b"
        preview={{
          id: "b",
          name: "Contact b",
          handles: ["+1555000b"],
          handleCount: 1,
          groups: ["Family"],
        }}
        onClose={() => {}}
      />,
    );

    const edit = screen.getByRole("button", { name: "Edit name" });
    expect(edit).toBeDisabled();

    resolveDetail(detail("b", { name: "Contact b", groups: ["Family"] }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Edit name" })).not.toBeDisabled();
    });
  });

  it("left-aligns identity headers and shows column resizers", async () => {
    get.mockResolvedValue(detail("a"));
    render(
      <ContactDrawer
        variant="docked"
        contactId="a"
        preview={{
          id: "a",
          name: "Contact a",
          handles: ["+1555000a"],
          groups: [],
        }}
        onClose={() => {}}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("grid", { name: "Contact handles" })).toBeTruthy();
    });

    const service = screen.getByRole("columnheader", { name: /Service/i });
    expect(service.className).toMatch(/text-center/);
    expect(service.className).not.toMatch(/text-left/);

    const firstSeen = screen.getByRole("columnheader", { name: /First Seen/i });
    expect(firstSeen.querySelector(".whitespace-nowrap")).toBeTruthy();
    expect(firstSeen.className).toMatch(/text-center/);
    expect(firstSeen.className).not.toMatch(/text-left/);
    const lastSeen = screen.getByRole("columnheader", { name: /Last Seen/i });
    expect(lastSeen.querySelector(".whitespace-nowrap")).toBeTruthy();
    expect(lastSeen.className).toMatch(/text-center/);
    expect(lastSeen.className).not.toMatch(/text-left/);

    const threads = screen.getByRole("columnheader", { name: /Threads/i });
    expect(threads.className).toMatch(/text-center/);
    expect(threads.className).not.toMatch(/text-right/);

    const direct = screen.getByRole("columnheader", { name: /Direct Messages/i });
    expect(direct.querySelector(".flex-col")).toBeTruthy();
    expect(direct.querySelector(".items-center")).toBeTruthy();
    expect(direct.className).toMatch(/text-center/);
    expect(direct.className).not.toMatch(/text-right/);
    const group = screen.getByRole("columnheader", { name: /Group Messages/i });
    expect(group.querySelector(".flex-col")).toBeTruthy();
    expect(group.querySelector(".items-center")).toBeTruthy();
    expect(group.className).toMatch(/text-center/);
    expect(group.className).not.toMatch(/text-right/);

    const table = screen.getByRole("grid", { name: "Contact handles" });
    expect(table.querySelectorAll(".cursor-col-resize").length).toBeGreaterThanOrEqual(8);

    const headers = screen.getAllByRole("columnheader");
    const groupIndex = headers.findIndex((h) => /Group Messages/i.test(h.textContent ?? ""));
    expect(groupIndex).toBe(headers.length - 1);
  });
});
