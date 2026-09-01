/** @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import AttachmentLightbox, { type LightboxItem } from "./AttachmentLightbox";

vi.mock("../lib/vaultApi", () => ({
  fetchAssetObjectUrl: vi.fn().mockResolvedValue("blob:mock-url"),
}));

const items: LightboxItem[] = [
  {
    attachment: {
      path: "first.png",
      original_name: "first.png",
      mime_type: "image/png",
      sha256: "aaa",
      is_sticker: false,
      transcription: null,
    },
    source: "demo",
  },
  {
    attachment: {
      path: "second.png",
      original_name: "second.png",
      mime_type: "image/png",
      sha256: "bbb",
      is_sticker: false,
      transcription: null,
    },
    source: "demo",
  },
];

describe("AttachmentLightbox", () => {
  it("moves to the next attachment when ArrowRight is pressed", async () => {
    const onNext = vi.fn();
    const onPrev = vi.fn();
    render(
      <AttachmentLightbox
        items={items}
        currentIndex={0}
        onClose={() => {}}
        onPrev={onPrev}
        onNext={onNext}
      />,
    );
    await screen.findByRole("img", { name: "first.png" });

    fireEvent.keyDown(document.body, { key: "ArrowRight" });

    expect(onNext).toHaveBeenCalledTimes(1);
    expect(onPrev).not.toHaveBeenCalled();
  });

  it("moves to the previous attachment when ArrowLeft is pressed", async () => {
    const onPrev = vi.fn();
    const onNext = vi.fn();
    render(
      <AttachmentLightbox
        items={items}
        currentIndex={1}
        onClose={() => {}}
        onPrev={onPrev}
        onNext={onNext}
      />,
    );
    await screen.findByRole("img", { name: "second.png" });

    fireEvent.keyDown(document.body, { key: "ArrowLeft" });

    expect(onPrev).toHaveBeenCalledTimes(1);
    expect(onNext).not.toHaveBeenCalled();
  });
});
