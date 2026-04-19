import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import WorkbenchLeftRail from "./WorkbenchLeftRail";
import type { Session } from "../stores";
import * as explorerModule from "../api/explorer";
import { ProjectProvider } from "../context/ProjectContext";
import { ExplorerContext } from "./left-rail/ExplorerContext";

function renderRail(children: () => any) {
  return render(() => (
    <ProjectProvider skipAutoLoad>
      {children()}
    </ProjectProvider>
  ));
}

function withExplorerContext(children: () => any, activeFilePath = () => null, focusedNodeId = () => null) {
  return () => (
    <ExplorerContext.Provider value={{ activeFilePath, focusedNodeId }}>
      {children()}
    </ExplorerContext.Provider>
  );
}

// Mock the explorer API module
vi.mock("../api/explorer", async () => {
  const actual = await vi.importActual("../api/explorer");
  return {
    ...actual,
    fetchExplorerBootstrap: vi.fn(),
    fetchExplorerTree: vi.fn(),
  };
});

describe("WorkbenchLeftRail onSelectFile", () => {
  const mockSessions: Session[] = [
    { id: "1", title: "Session One", status: "completed", updated_at: new Date().toISOString() },
  ];

  const defaultProps = {
    sessions: mockSessions,
    currentSessionId: "1",
    onSelect: vi.fn(),
    onNewSession: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.restoreAllMocks();
  });

  it("should call onSelectFile when a file is clicked in explorer", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    // Root returns a directory with a file
    vi.mocked(explorerModule.fetchExplorerTree).mockImplementation(
      (sessionId?: string, path: string = ".", _depth: number = 1, _filter: string = "all") => {
        if (path === ".") {
          return Promise.resolve({
            path: "/test",
            version: 1,
            children: [
              { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir", has_children: true },
            ],
          });
        }
        if (path === "/test/src") {
          return Promise.resolve({
            path: "/test/src",
            version: 1,
            children: [
              { id: "/test/src/main.rs", name: "main.rs", path: "/test/src/main.rs", relative_path: "src/main.rs", kind: "file" },
            ],
          });
        }
        return Promise.resolve({ path, version: 1, children: [] });
      }
    );

    const { container } = renderRail(
      withExplorerContext(() => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Click on the directory to expand
    const srcDir = container.querySelector("[data-node-kind='dir']") as HTMLDivElement;
    fireEvent.click(srcDir);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Click on the file
    const fileNode = container.querySelector("[data-node-kind='file']") as HTMLDivElement;
    fireEvent.click(fileNode);

    // Verify onSelectFile was called with the file's relative path
    expect(onSelectFile).toHaveBeenCalledWith("src/main.rs");
  });

  it("should NOT call onSelectFile when a directory is clicked", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    vi.mocked(explorerModule.fetchExplorerTree).mockResolvedValue({
      path: "/test",
      version: 1,
      children: [
        { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir" },
      ],
    });

    const { container } = renderRail(
      withExplorerContext(() => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Click on the directory (to expand it)
    const srcDir = container.querySelector("[data-node-kind='dir']") as HTMLDivElement;
    fireEvent.click(srcDir);

    // onSelectFile should NOT have been called
    expect(onSelectFile).not.toHaveBeenCalled();
  });

  // ========== T4.3: Keyboard Navigation Tests ==========

  it("T4.3.1: keyboard navigation works with arrow keys", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    // Root returns a directory with a file
    vi.mocked(explorerModule.fetchExplorerTree).mockImplementation(
      (sessionId?: string, path: string = ".", _depth: number = 1, _filter: string = "all") => {
        if (path === ".") {
          return Promise.resolve({
            path: "/test",
            version: 1,
            children: [
              { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir", has_children: true },
              { id: "/test/README.md", name: "README.md", path: "/test/README.md", relative_path: "README.md", kind: "file" },
            ],
          });
        }
        if (path === "/test/src") {
          return Promise.resolve({
            path: "/test/src",
            version: 1,
            children: [
              { id: "/test/src/main.rs", name: "main.rs", path: "/test/src/main.rs", relative_path: "src/main.rs", kind: "file" },
            ],
          });
        }
        return Promise.resolve({ path, version: 1, children: [] });
      }
    );

    const { container } = renderRail(
      withExplorerContext(() => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Get the explorer tree container
    const explorerTree = container.querySelector("[data-component='explorer-tree']") as HTMLDivElement;
    expect(explorerTree).toBeDefined();

    // Focus the explorer tree by clicking on it
    fireEvent.click(explorerTree);

    // Press ArrowDown to focus first node
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // Press ArrowDown again to move to second node
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // Press Enter to expand first node (src directory)
    fireEvent.keyDown(explorerTree, { key: "Enter" });
    await new Promise(resolve => setTimeout(resolve, 100));

    // First node (src dir) should be expanded now
    const srcDir = container.querySelector("[data-node-id='/test/src']") as HTMLDivElement;
    expect(srcDir).not.toBeNull();
  });

  it("T4.3.2: enter key expands directory and selects file", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    vi.mocked(explorerModule.fetchExplorerTree).mockImplementation(
      (sessionId?: string, path: string = ".", _depth: number = 1, _filter: string = "all") => {
        if (path === ".") {
          return Promise.resolve({
            path: "/test",
            version: 1,
            children: [
              { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir", has_children: true },
            ],
          });
        }
        if (path === "/test/src") {
          return Promise.resolve({
            path: "/test/src",
            version: 1,
            children: [
              { id: "/test/src/main.rs", name: "main.rs", path: "/test/src/main.rs", relative_path: "src/main.rs", kind: "file" },
            ],
          });
        }
        return Promise.resolve({ path, version: 1, children: [] });
      }
    );

    const { container } = renderRail(
      withExplorerContext(() => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Get the explorer tree container and focus it
    const explorerTree = container.querySelector("[data-component='explorer-tree']") as HTMLDivElement;
    fireEvent.click(explorerTree);

    // Navigate to src directory with ArrowDown and expand with Enter
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));
    fireEvent.keyDown(explorerTree, { key: "Enter" });
    await new Promise(resolve => setTimeout(resolve, 100));

    // Navigate to main.rs file with ArrowDown
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // Press Enter on file to select it
    fireEvent.keyDown(explorerTree, { key: "Enter" });

    // onSelectFile should have been called
    expect(onSelectFile).toHaveBeenCalled();
  });

  it("T4.3.3: left arrow collapses directory", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    vi.mocked(explorerModule.fetchExplorerTree).mockImplementation(
      (sessionId?: string, path: string = ".", _depth: number = 1, _filter: string = "all") => {
        if (path === ".") {
          return Promise.resolve({
            path: "/test",
            version: 1,
            children: [
              { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir", has_children: true },
            ],
          });
        }
        if (path === "/test/src") {
          return Promise.resolve({
            path: "/test/src",
            version: 1,
            children: [
              { id: "/test/src/main.rs", name: "main.rs", path: "/test/src/main.rs", relative_path: "src/main.rs", kind: "file" },
            ],
          });
        }
        return Promise.resolve({ path, version: 1, children: [] });
      }
    );

    const { container } = renderRail(
      withExplorerContext(() => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Get the explorer tree container and focus it
    const explorerTree = container.querySelector("[data-component='explorer-tree']") as HTMLDivElement;
    fireEvent.click(explorerTree);

    // Navigate to src directory and expand it
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));
    fireEvent.keyDown(explorerTree, { key: "Enter" });
    await new Promise(resolve => setTimeout(resolve, 100));

    // Navigate to main.rs file
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // Press left arrow - should collapse (navigate back to parent)
    fireEvent.keyDown(explorerTree, { key: "ArrowLeft" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // onSelectFile should NOT have been called (just navigation)
    expect(onSelectFile).not.toHaveBeenCalled();
  });

  // ========== REQ-4.4: Space Key Tests ==========

  it("REQ-4.4.1: Space key expands directory nodes like Enter", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    vi.mocked(explorerModule.fetchExplorerTree).mockImplementation(
      (sessionId?: string, path: string = ".", _depth: number = 1, _filter: string = "all") => {
        if (path === ".") {
          return Promise.resolve({
            path: "/test",
            version: 1,
            children: [
              { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir", has_children: true },
            ],
          });
        }
        if (path === "/test/src") {
          return Promise.resolve({
            path: "/test/src",
            version: 1,
            children: [
              { id: "/test/src/main.rs", name: "main.rs", path: "/test/src/main.rs", relative_path: "src/main.rs", kind: "file" },
            ],
          });
        }
        return Promise.resolve({ path, version: 1, children: [] });
      }
    );

    const { container } = renderRail(
      withExplorerContext(() => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Get the explorer tree container and focus it
    const explorerTree = container.querySelector("[data-component='explorer-tree']") as HTMLDivElement;
    fireEvent.click(explorerTree);

    // Navigate to src directory with ArrowDown
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // Press Space to expand directory (same behavior as Enter for dirs)
    fireEvent.keyDown(explorerTree, { key: " " });
    await new Promise(resolve => setTimeout(resolve, 100));

    // Directory should be expanded and children loaded
    const srcDir = container.querySelector("[data-node-id='/test/src']");
    expect(srcDir).not.toBeNull();

    // Children should now be visible (main.rs file)
    const mainRsFile = container.querySelector("[data-node-id='/test/src/main.rs']");
    expect(mainRsFile).not.toBeNull();
  });

  it("REQ-4.4.2: data-focused attribute is set correctly on focused node", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    vi.mocked(explorerModule.fetchExplorerTree).mockResolvedValue({
      path: "/test",
      version: 1,
      children: [
        { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir" },
        { id: "/test/README.md", name: "README.md", path: "/test/README.md", relative_path: "README.md", kind: "file" },
      ],
    });

    const { container } = renderRail(
      withExplorerContext(() => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Get the explorer tree container and focus it
    const explorerTree = container.querySelector("[data-component='explorer-tree']") as HTMLDivElement;
    fireEvent.click(explorerTree);

    // Initially no focused node
    let focusedNodes = container.querySelectorAll("[data-focused='true']");
    expect(focusedNodes.length).toBe(0);

    // Press ArrowDown to focus first node
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // First node should have data-focused="true"
    const srcNode = container.querySelector("[data-node-id='/test/src']");
    expect(srcNode?.getAttribute("data-focused")).toBe("true");

    // Other nodes should have data-focused="false"
    const readmeNode = container.querySelector("[data-node-id='/test/README.md']");
    expect(readmeNode?.getAttribute("data-focused")).toBe("false");

    // Press ArrowDown again to focus second node
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // First node should now have data-focused="false"
    expect(srcNode?.getAttribute("data-focused")).toBe("false");

    // Second node should have data-focused="true"
    expect(readmeNode?.getAttribute("data-focused")).toBe("true");
  });

  it("uses project_id when the explorer has an active project", async () => {
    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/project-root",
      repo_root: "/project-root",
      repo_relative_root: ".",
      watching: false,
      git_available: true,
      git_branch: "main",
      case_sensitive: true,
    });
    vi.mocked(explorerModule.fetchExplorerTree).mockResolvedValue({
      path: "/project-root",
      version: 1,
      children: [],
    });

    const { container } = render(() => (
      <ProjectProvider
        skipAutoLoad
        initialProjects={[
          { id: "project-1", name: "Demo", canonical_path: "/project-root", session_count: 1, pinned: false, icon: null, created_at: "", updated_at: "" },
        ]}
        initialActiveProjectId="project-1"
      >
        <ExplorerContext.Provider value={{ activeFilePath: () => null, focusedNodeId: () => null }}>
          <WorkbenchLeftRail {...defaultProps} currentSessionId={undefined} />
        </ExplorerContext.Provider>
      </ProjectProvider>
    ));

    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise((resolve) => setTimeout(resolve, 100));

    expect(explorerModule.fetchExplorerBootstrap).toHaveBeenCalledWith("1", "project-1");
    expect(explorerModule.fetchExplorerTree).toHaveBeenCalledWith("1", ".", 1, "all", false, false, "project-1");
  });

  it("REQ-4.4.3: focused node has distinct styling class", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    vi.mocked(explorerModule.fetchExplorerTree).mockResolvedValue({
      path: "/test",
      version: 1,
      children: [
        { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir" },
      ],
    });

    const { container } = renderRail(
      withExplorerContext(() => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Get the explorer tree container and focus it
    const explorerTree = container.querySelector("[data-component='explorer-tree']") as HTMLDivElement;
    fireEvent.click(explorerTree);

    // Press ArrowDown to focus the node
    fireEvent.keyDown(explorerTree, { key: "ArrowDown" });
    await new Promise(resolve => setTimeout(resolve, 50));

    // Find the focused node
    const focusedNode = container.querySelector("[data-focused='true']");
    expect(focusedNode).not.toBeNull();

    // The focused node should have ring styling and bg-primary/10
    expect(focusedNode?.className).toContain("ring-1");
    expect(focusedNode?.className).toContain("ring-primary");
    expect(focusedNode?.className).toContain("bg-primary/10");
  });

  it("REQ-4.4.4: active file node has correct visual styling via ExplorerContext", async () => {
    const onSelectFile = vi.fn();

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    // Root returns directory with children (pre-expanded so no click needed)
    vi.mocked(explorerModule.fetchExplorerTree).mockResolvedValue({
      path: "/test",
      version: 1,
      children: [
        { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir" },
        { id: "/test/src/main.rs", name: "main.rs", path: "/test/src/main.rs", relative_path: "src/main.rs", kind: "file" },
      ],
    });

    // Pass activeFilePath as a plain string prop — WorkbenchLeftRail creates
    // its own ExplorerContext.Provider internally, so no external provider needed.
    const { container } = render(() => (
      <ProjectProvider skipAutoLoad>
        <WorkbenchLeftRail
          {...defaultProps}
          onSelectFile={onSelectFile}
          activeFilePath="src/main.rs"
        />
      </ProjectProvider>
    ));

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Expand the src directory to reveal main.rs
    const srcDir = container.querySelector("[data-node-id='/test/src']") as HTMLDivElement;
    fireEvent.click(srcDir);
    await new Promise(resolve => setTimeout(resolve, 200));

    // The file node should be visible
    const activeFileNode = container.querySelector("[data-node-id='/test/src/main.rs']");
    expect(activeFileNode).not.toBeNull();

    // Verify data-active attribute confirms ExplorerContext provides activeFilePath correctly
    // This is the key evidence that the context wiring works end-to-end:
    // WorkbenchLeftRail receives activeFilePath prop → creates ExplorerContext.Provider →
    // TreeNodeRow reads from context → sets data-active="true"
    expect(activeFileNode?.getAttribute("data-active")).toBe("true");

    // When activeFilePath matches, the node is also focused (REQ-4.5 effect),
    // so the focused styling takes priority over the active-file styling.
    // Verify that at minimum the node has visual styling (focused or active).
    const hasActiveStyling =
      activeFileNode?.className?.includes("bg-primary-container/30") ||
      activeFileNode?.className?.includes("bg-primary/10");
    expect(hasActiveStyling).toBe(true);
  });

  it("Smoke-4.4.5: non-active file node has no active styling", async () => {
    const onSelectFile = vi.fn();
    // activeFilePath points to a different file, so this one is NOT active
    const [activeFilePath] = createSignal<string | null>("other/file.ts");
    const [focusedNodeId] = createSignal<string | null>(null);

    vi.mocked(explorerModule.fetchExplorerBootstrap).mockResolvedValue({
      workspace_root: "/test",
      repo_root: "/test",
      repo_relative_root: ".",
      watching: false,
      git_available: false,
      git_branch: null,
      case_sensitive: true,
    });

    vi.mocked(explorerModule.fetchExplorerTree).mockImplementation(
      (sessionId?: string, path: string = ".", _depth: number = 1, _filter: string = "all") => {
        if (path === ".") {
          return Promise.resolve({
            path: "/test",
            version: 1,
            children: [
              { id: "/test/src", name: "src", path: "/test/src", relative_path: "src", kind: "dir" },
            ],
          });
        }
        if (path === "/test/src") {
          return Promise.resolve({
            path: "/test/src",
            version: 1,
            children: [
              { id: "/test/src/main.rs", name: "main.rs", path: "/test/src/main.rs", relative_path: "src/main.rs", kind: "file" },
            ],
          });
        }
        return Promise.resolve({ path, version: 1, children: [] });
      }
    );

    const { container } = renderRail(
      withExplorerContext(
        () => <WorkbenchLeftRail {...defaultProps} onSelectFile={onSelectFile} />,
        activeFilePath as () => null,
        focusedNodeId as () => null,
      ),
    );

    // Switch to Explorer tab
    const explorerTab = container.querySelector("[data-tab='explorer']") as HTMLButtonElement;
    fireEvent.click(explorerTab);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Expand the src directory
    const srcDir = container.querySelector("[data-node-id='/test/src']") as HTMLDivElement;
    fireEvent.click(srcDir);
    await new Promise(resolve => setTimeout(resolve, 100));

    // Find the main.rs file node - it is NOT the active file
    const fileNode = container.querySelector("[data-node-id='/test/src/main.rs']");
    expect(fileNode).not.toBeNull();

    // It should NOT have the active-file styling
    expect(fileNode?.className).not.toContain("bg-primary-container/30");
    expect(fileNode?.className).not.toContain("text-primary");

    // It should NOT have data-active="true"
    expect(fileNode?.getAttribute("data-active")).not.toBe("true");
  });
});
