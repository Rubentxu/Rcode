import { For, Show } from "solid-js";
import type { ExplorerFilter } from "../../api/explorer";

export interface FilterCounts {
  changed: number;
  staged: number;
  untracked: number;
  conflicted: number;
}

export function FilterBar(props: {
  activeFilter: ExplorerFilter;
  onFilterChange: (filter: ExplorerFilter) => void;
  counts: FilterCounts;
}) {
  const filters: { key: ExplorerFilter; label: string }[] = [
    { key: "all", label: "All" },
    { key: "changed", label: "Changed" },
    { key: "staged", label: "Staged" },
    { key: "untracked", label: "Untracked" },
    { key: "conflicted", label: "Conflicted" },
  ];

  return (
    <div class="flex items-center gap-1 px-2 py-1 border-b border-outline-variant/20 overflow-x-auto custom-scrollbar">
      <For each={filters}>
        {(filter) => {
          const count = () => {
            switch (filter.key) {
              case "changed": return props.counts.changed;
              case "staged": return props.counts.staged;
              case "untracked": return props.counts.untracked;
              case "conflicted": return props.counts.conflicted;
              default: return null;
            }
          };

          return (
            <button
              onClick={() => props.onFilterChange(filter.key)}
              class={`flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium transition-all whitespace-nowrap ${
                props.activeFilter === filter.key
                  ? "bg-primary-container text-on-primary-container"
                  : "text-outline hover:bg-surface-container-high hover:text-on-surface"
              }`}
            >
              <span>{filter.label}</span>
              <Show when={count() !== null && count()! > 0}>
                <span class={`px-1 rounded-full text-[9px] ${
                  props.activeFilter === filter.key 
                    ? "bg-on-primary-container/20" 
                    : "bg-surface-container-high"
                }`}>
                  {count()}
                </span>
              </Show>
            </button>
          );
        }}
      </For>
    </div>
  );
}
