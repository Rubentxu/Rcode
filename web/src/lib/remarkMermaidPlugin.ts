import { visit } from "unist-util-visit";
import type { Root, Element } from "hast";
import type { Plugin } from "unified";

/**
 * Rehype plugin that transforms Mermaid code blocks into stable placeholders
 * for lazy client-side rendering.
 *
 * Transforms:
 * ```mermaid
 * graph TD
 *   A --> B
 * ```
 *
 * Into:
 * <pre class="mermaid">graph TD\n  A --> B</pre>
 */
export const remarkMermaidPlugin: Plugin<[], Root> = () => {
  return (tree: Root) => {
    visit(tree, "element", (node: Element, index, parent?: Element | Root) => {
      // Look for <pre><code class="language-mermaid">...</code></pre>
      if (
        node.tagName === "pre" &&
        parent &&
        typeof index === "number" &&
        "children" in parent
      ) {
        const codeChild = node.children.find(
          (child): child is Element => child.type === "element" && child.tagName === "code"
        );
        const classes = codeChild?.properties?.className;
        if (!codeChild || !Array.isArray(classes)) {
          return;
        }

        const isMermaid = classes.some(
          (className) => typeof className === "string" && className.startsWith("language-mermaid")
        );
        if (!isMermaid) {
          return;
        }

        const mermaidCode = codeChild.children
          .filter((child) => child.type === "text")
          .map((child) => child.value)
          .join("")
          .trim();

        const mermaidPre: Element = {
          type: "element",
          tagName: "pre",
          properties: {
            className: ["mermaid"],
            "data-mermaid-source": mermaidCode,
            "data-mermaid-status": "pending",
          },
          children: [
            {
              type: "text",
              value: mermaidCode,
            },
          ],
        };

        parent.children.splice(index, 1, mermaidPre);
      }
    });
  };
};

export default remarkMermaidPlugin;
