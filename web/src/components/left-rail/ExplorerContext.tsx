import { createContext, useContext, type Accessor } from "solid-js";

export interface ExplorerContextValue {
  activeFilePath: Accessor<string | null>;
  focusedNodeId: Accessor<string | null>;
}

const ExplorerContext = createContext<ExplorerContextValue | undefined>(undefined);

export function useExplorerContext(): ExplorerContextValue {
  const ctx = useContext(ExplorerContext);
  if (!ctx) {
    throw new Error("useExplorerContext must be used within ExplorerProvider");
  }
  return ctx;
}

export { ExplorerContext };
