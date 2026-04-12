/**
 * T3.2: Virtual scrolling composable using @tanstack/solid-virtual
 * 
 * Provides efficient rendering of large lists by only rendering visible items.
 * Configured with overscan=5 for smooth scrolling and estimateSize=200
 * as a reasonable default for chat message turns.
 */

import { createVirtualizer, type Virtualizer } from "@tanstack/solid-virtual";
import { type Accessor, createEffect, onMount, onCleanup } from "solid-js";

export interface VirtualScrollOptions {
  /** Number of items to render beyond visible area */
  overscan?: number;
  /** Estimated size of each item in pixels */
  estimateSize?: number;
  /** Gap between items */
  gap?: number;
}

export interface VirtualScrollReturn<T> {
  /** The virtualizer instance */
  virtualizer: Virtualizer<HTMLElement, T>;
  /** Total size of all items (reactive) */
  totalSize: Accessor<number>;
  /** Array of virtual items to render (reactive) */
  virtualItems: Accessor<ReturnType<Virtualizer<HTMLElement, T>["getVirtualItems"]>>;
  /** Scroll to a specific index */
  scrollToIndex: (index: number, options?: { align?: "start" | "center" | "end" | "auto" }) => void;
  /** Scroll to a specific offset */
  scrollToOffset: (offset: number) => void;
  /** Measure an element at the given index */
  measureElement: (element: HTMLElement | null) => void;
  /** Re-measure all elements */
  measure: () => void;
}

/**
 * Creates a virtualized scrolling setup for large lists.
 * 
 * @param getScrollElement - Function that returns the scroll container element
 * @param count - Accessor that returns the total number of items
 * @param options - Configuration options
 */
export function useVirtualScroll<T>(
  getScrollElement: () => HTMLElement | undefined,
  count: Accessor<number>,
  options: VirtualScrollOptions = {}
): VirtualScrollReturn<T> {
  const { overscan = 5, estimateSize = 200, gap = 0 } = options;

  // The virtualizer is created once and kept as a stable reference
  // It automatically tracks reactive dependencies in its getter functions
  let virtualizer: Virtualizer<HTMLElement, T> | undefined;
  let scrollElement: HTMLElement | undefined;

  // Reactive accessors that will update when the virtualizer's internal state changes
  const totalSize = (): number => virtualizer?.getTotalSize() ?? 0;
  const virtualItems = (): ReturnType<Virtualizer<HTMLElement, T>["getVirtualItems"]> => 
    virtualizer?.getVirtualItems() ?? [];

  // Initialize the virtualizer when scroll element is available
  const initVirtualizer = () => {
    scrollElement = getScrollElement();
    if (!scrollElement) return;

    virtualizer = createVirtualizer({
      get count() {
        return count();
      },
      getScrollElement: () => scrollElement,
      estimateSize: () => estimateSize,
      overscan,
      gap,
      measureElement: (element: HTMLElement | null) => {
        if (!element) return 0;
        return element.getBoundingClientRect().height;
      },
    });
  };

  // Initialize on first call
  if (!virtualizer) {
    initVirtualizer();
  }

  const scrollToIndex = (index: number, opts?: { align?: "start" | "center" | "end" | "auto" }) => {
    virtualizer?.scrollToIndex(index, opts);
  };

  const scrollToOffset = (offset: number) => {
    virtualizer?.scrollToOffset(offset);
  };

  const measureElement = (element: HTMLElement | null) => {
    virtualizer?.measureElement(element);
  };

  const measure = () => {
    virtualizer?.measure();
  };

  return {
    get virtualizer() {
      return virtualizer!;
    },
    totalSize,
    virtualItems,
    scrollToIndex,
    scrollToOffset,
    measureElement,
    measure,
  };
}
