import { render } from "@solidjs/testing-library";
import { describe, it, expect } from "vitest";
import { AttachmentPart } from "./AttachmentPart";

type AttachmentPartProps = Parameters<typeof AttachmentPart>[0];

describe("AttachmentPart", () => {
  const createProps = (overrides: Partial<AttachmentPartProps> = {}): AttachmentPartProps => ({
    id: "test-attachment",
    name: "test-file",
    mime_type: "application/octet-stream",
    content: null,
    ...overrides,
  });

  describe("image/* mime types", () => {
    it("renders img element for image/png mime type", () => {
      const props = createProps({
        mime_type: "image/png",
        content: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const img = container.querySelector("img");
      expect(img).not.toBeNull();
      expect(img?.tagName).toBe("IMG");
      expect(img?.getAttribute("alt")).toBe(props.name);
      expect(img?.src).toContain("data:image/png;base64,");
    });

    it("renders img element for image/jpeg mime type", () => {
      const props = createProps({
        mime_type: "image/jpeg",
        content: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const img = container.querySelector("img");
      expect(img).not.toBeNull();
      expect(img?.getAttribute("src")).toContain("data:image/jpeg;base64,");
    });

    it("renders img element for image/webp mime type", () => {
      const props = createProps({
        mime_type: "image/webp",
        content: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const img = container.querySelector("img");
      expect(img).not.toBeNull();
      expect(img?.getAttribute("src")).toContain("data:image/webp;base64,");
    });

    it("renders img element for image/gif mime type", () => {
      const props = createProps({
        mime_type: "image/gif",
        content: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const img = container.querySelector("img");
      expect(img).not.toBeNull();
      expect(img?.getAttribute("src")).toContain("data:image/gif;base64,");
    });
  });

  describe("image/* over 5MB", () => {
    it("renders file card for image over 5MB decoded size", () => {
      // Need ~7MB of base64 chars to decode to >5MB bytes (4 base64 chars → 3 bytes)
      const largeContent = "A".repeat(7 * 1024 * 1024);
      const props = createProps({
        mime_type: "image/png",
        content: largeContent,
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const img = container.querySelector("img");
      expect(img).toBeNull();
      const card = container.querySelector(".attachment-card");
      expect(card).not.toBeNull();
    });

    it("renders img element for image under 5MB", () => {
      const smallContent = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
      const props = createProps({
        mime_type: "image/png",
        content: smallContent,
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const img = container.querySelector("img");
      expect(img).not.toBeNull();
    });
  });

  describe("audio/* mime types", () => {
    it("renders audio element with controls for audio/mpeg", () => {
      const props = createProps({
        mime_type: "audio/mpeg",
        content: "SUQzBAAAAAAAI1RTU0UAAAAPAAADTGF2ZjU4Ljc2LjEwMAAAAAAAAAAAAAAA//tQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWGluZwAAAA8AAAACAAABhgC7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7",
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const audio = container.querySelector("audio");
      expect(audio).not.toBeNull();
      expect(audio?.getAttribute("controls")).toBe("");
      expect(audio?.src).toContain("data:audio/mpeg;base64,");
    });

    it("renders audio element for audio/wav mime type", () => {
      const props = createProps({
        mime_type: "audio/wav",
        content: "SUQzBAAAAAAAI1RTU0UAAAAPAAADTGF2ZjU4Ljc2LjEwMAAAAAAAAAAAAAAA//tQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWGluZwAAAA8AAAACAAABhgC7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7",
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const audio = container.querySelector("audio");
      expect(audio).not.toBeNull();
    });
  });

  describe("other mime types", () => {
    it("renders file card for application/pdf", () => {
      const props = createProps({
        mime_type: "application/pdf",
        content: "JVBERi0xLjQKJeLjz9MKNSAwIG9iago8PAovRmlsdGVyIC9GbGF0ZURlY29kZQo=",
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const card = container.querySelector(".attachment-card");
      expect(card).not.toBeNull();
      const img = container.querySelector("img");
      const audio = container.querySelector("audio");
      expect(img).toBeNull();
      expect(audio).toBeNull();
    });

    it("renders file card for text/plain", () => {
      const props = createProps({
        mime_type: "text/plain",
        content: "SGVsbG8gV29ybGQ=",
      });
      const { container } = render(() => <AttachmentPart {...props} />);
      const card = container.querySelector(".attachment-card");
      expect(card).not.toBeNull();
    });
  });
});
