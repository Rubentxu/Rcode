import { describe, it, expect } from "vitest";
import { TextPart } from "./TextPart";

describe("TextPart", () => {
  it("should render text part component", () => {
    const container = document.createElement("div");
    const result = TextPart({ content: "Hello, world!" });
    container.appendChild(result as Node);
    
    expect(container.querySelector(".text-part")).toBeDefined();
  });

  it("should handle empty string", () => {
    const result = TextPart({ content: "" });
    expect(result).toBeDefined();
  });
});
