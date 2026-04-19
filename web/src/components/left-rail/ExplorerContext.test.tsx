import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { useExplorerContext, ExplorerContext } from "./ExplorerContext";

describe("ExplorerContext", () => {
  describe("useExplorerContext", () => {
    it("throws when called outside of ExplorerProvider", () => {
      // This component attempts to use useExplorerContext without a provider
      const ConsumerOutsideProvider = () => {
        useExplorerContext();
        return <div>Test</div>;
      };

      expect(() => render(() => <ConsumerOutsideProvider />)).toThrow(
        "useExplorerContext must be used within ExplorerProvider"
      );
    });

    it("returns the context value when called within ExplorerProvider", () => {
      const [activeFilePath, setActiveFilePath] = createSignal<string | null>("src/main.rs");
      const [focusedNodeId, setFocusedNodeId] = createSignal<string | null>("/test/src");

      const ConsumerInsideProvider = () => {
        const ctx = useExplorerContext();
        return (
          <div>
            <span data-testid="active-file">{ctx.activeFilePath()}</span>
            <span data-testid="focused-node">{ctx.focusedNodeId()}</span>
          </div>
        );
      };

      const { getByTestId } = render(() => (
        <ExplorerContext.Provider value={{ activeFilePath, focusedNodeId }}>
          <ConsumerInsideProvider />
        </ExplorerContext.Provider>
      ));

      expect(getByTestId("active-file").textContent).toBe("src/main.rs");
      expect(getByTestId("focused-node").textContent).toBe("/test/src");
    });

    it("context values are reactive", () => {
      const [activeFilePath, setActiveFilePath] = createSignal<string | null>(null);
      const [focusedNodeId, setFocusedNodeId] = createSignal<string | null>(null);

      const ConsumerInsideProvider = () => {
        const ctx = useExplorerContext();
        return (
          <div>
            <span data-testid="active-file">{ctx.activeFilePath() ?? "null"}</span>
            <span data-testid="focused-node">{ctx.focusedNodeId() ?? "null"}</span>
          </div>
        );
      };

      const { getByTestId } = render(() => (
        <ExplorerContext.Provider value={{ activeFilePath, focusedNodeId }}>
          <ConsumerInsideProvider />
        </ExplorerContext.Provider>
      ));

      expect(getByTestId("active-file").textContent).toBe("null");
      expect(getByTestId("focused-node").textContent).toBe("null");

      // Update the signals
      setActiveFilePath("src/index.ts");
      setFocusedNodeId("/test/src/index");

      expect(getByTestId("active-file").textContent).toBe("src/index.ts");
      expect(getByTestId("focused-node").textContent).toBe("/test/src/index");
    });

    it("provides accessors that return correct types", () => {
      const [activeFilePath] = createSignal<string | null>("test.rs");
      const [focusedNodeId] = createSignal<string | null>(null);

      const ConsumerInsideProvider = () => {
        const ctx = useExplorerContext();
        // Verify types are correct by using them
        const activePath: string | null = ctx.activeFilePath();
        const focusedId: string | null = ctx.focusedNodeId();
        return (
          <div>
            <span data-testid="type-check">
              {typeof activePath === "string" && focusedId === null ? "ok" : "fail"}
            </span>
          </div>
        );
      };

      const { getByTestId } = render(() => (
        <ExplorerContext.Provider value={{ activeFilePath, focusedNodeId }}>
          <ConsumerInsideProvider />
        </ExplorerContext.Provider>
      ));

      expect(getByTestId("type-check").textContent).toBe("ok");
    });
  });
});
