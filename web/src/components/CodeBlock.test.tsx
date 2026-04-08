import { describe, it, expect } from "vitest";
import { renderMarkdownToHtml } from "./MarkdownRenderer";

// MCR-3: Language badge on code blocks
describe("MCR-3: Code block language badge", () => {
  it("should add data-language attribute to code blocks with language", async () => {
    const html = await renderMarkdownToHtml("```rust\nfn main() {}\n```");
    expect(html).toContain('data-language="rust"');
  });

  it("should add data-language attribute for javascript", async () => {
    const html = await renderMarkdownToHtml("```javascript\nconsole.log('hi');\n```");
    expect(html).toContain('data-language="javascript"');
  });

  it("should add data-language attribute for python", async () => {
    const html = await renderMarkdownToHtml("```python\nprint('hello')\n```");
    expect(html).toContain('data-language="python"');
  });

  it("should add data-language attribute for bash", async () => {
    const html = await renderMarkdownToHtml("```bash\necho hi\n```");
    expect(html).toContain('data-language="bash"');
  });

  it("should have pre element for unknown language", async () => {
    const html = await renderMarkdownToHtml("```\nplain text code\n```");
    expect(html).toContain("<pre");
  });
});

// MCR-4: Code block copy button - tested via enhancement in MarkdownRenderer
// The HTML output includes code that can be copied via clipboard API
describe("MCR-4: Code block copy button", () => {
  it("should include code content in pre element for copy extraction", async () => {
    // The code content should be present in the HTML for copy functionality
    const html = await renderMarkdownToHtml("```rust\nfn main() {}\n```");
    // Create a temporary element to extract textContent
    const div = document.createElement("div");
    div.innerHTML = html;
    const pre = div.querySelector("pre[data-language]");
    const textContent = pre?.textContent || "";
    expect(textContent).toContain("fn main");
  });

  it("should include multi-line code content", async () => {
    const code = "```javascript\nconst x = 1;\nconst y = 2;\nconsole.log(x + y);\n```";
    const html = await renderMarkdownToHtml(code);
    const div = document.createElement("div");
    div.innerHTML = html;
    const pre = div.querySelector("pre[data-language]");
    const textContent = pre?.textContent || "";
    expect(textContent).toContain("const x = 1");
    expect(textContent).toContain("const y = 2");
    expect(textContent).toContain("console.log");
  });

  it("should preserve special characters in code", async () => {
    const html = await renderMarkdownToHtml("```\nconst arr = [1, 2, 3];\nconst obj = { a: 1 };\n```");
    const div = document.createElement("div");
    div.innerHTML = html;
    const pre = div.querySelector("pre");
    const textContent = pre?.textContent || "";
    expect(textContent).toContain("arr");
    expect(textContent).toContain("obj");
  });
});
