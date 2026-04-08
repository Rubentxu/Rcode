import { describe, it, expect } from "vitest";
import { AttachmentPart } from "./AttachmentPart";

describe("AttachmentPart", () => {
  it("should display file name", () => {
    const container = document.createElement("div");
    const result = AttachmentPart({ id: "att_123", name: "document.pdf", mime_type: "application/pdf", content: null });
    container.appendChild(result as Node);
    
    const name = container.querySelector(".attachment-name");
    expect(name?.textContent).toBe("document.pdf");
  });

  it("should display mime type", () => {
    const container = document.createElement("div");
    const result = AttachmentPart({ id: "att_123", name: "image.png", mime_type: "image/png", content: null });
    container.appendChild(result as Node);
    
    const meta = container.querySelector(".attachment-meta");
    expect(meta?.textContent).toContain("image/png");
  });

  it("should show correct icon for image files", () => {
    const container = document.createElement("div");
    const result = AttachmentPart({ id: "att_123", name: "photo.jpg", mime_type: "image/jpeg", content: null });
    container.appendChild(result as Node);
    
    const icon = container.querySelector(".attachment-icon");
    expect(icon?.textContent).toBe("🖼️");
  });

  it("should show correct icon for PDF files", () => {
    const container = document.createElement("div");
    const result = AttachmentPart({ id: "att_456", name: "report.pdf", mime_type: "application/pdf", content: null });
    container.appendChild(result as Node);
    
    const icon = container.querySelector(".attachment-icon");
    expect(icon?.textContent).toBe("📄");
  });

  it("should show download button", () => {
    const container = document.createElement("div");
    const result = AttachmentPart({ id: "att_789", name: "file.txt", mime_type: "text/plain", content: null });
    container.appendChild(result as Node);
    
    const downloadBtn = container.querySelector(".attachment-download");
    expect(downloadBtn).toBeDefined();
  });
});
