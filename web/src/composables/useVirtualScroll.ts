/**
 * Virtual scrolling composable with pretext height pre-computation.
 *
 * Uses @chenglou/pretext to estimate turn heights without DOM measurement,
 * then @tanstack/solid-virtual to virtualize the list. Dynamic measurement
 * via measureElement corrects any estimate drift.
 */

import { createVirtualizer, type Virtualizer } from "@tanstack/solid-virtual";
import { type Accessor } from "solid-js";
import { prepare, layout, clearCache } from "@chenglou/pretext";

export interface VirtualScrollOptions {
  /** Number of items to render beyond visible area */
  overscan?: number;
  /** Estimated size of each item in pixels (fallback when pretext unavailable) */
  estimateSize?: number;
  /** Gap between items in pixels */
  gap?: number;
  /** Container width in pixels (for pretext line calculation) */
  containerWidth?: number;
  /** Font string for pretext measurement (e.g., "14px system-ui") */
  font?: string;
  /** Line height in pixels for pretext */
  lineHeight?: number;
  /** Padding per turn (top + bottom) added to text height */
  turnPadding?: number;
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
  /** Measure an element at the given index (corrects estimate) */
  measureElement: (element: HTMLElement | null) => void;
  /** Re-measure all elements */
  measure: () => void;
  /** Get the current estimate for an item's height (for debugging) */
  getEstimate: (index: number) => number;
}

/**
 * Estimates the rendered height of a text string using pretext.
 * Returns height in pixels WITHOUT touching the DOM.
 */
function estimateTextHeight(
  text: string,
  font: string,
  maxWidth: number,
  lineHeight: number,
  padding: number,
): number {
  if (!text?.trim()) return padding;
  try {
    const prepared = prepare(text, font);
    const result = layout(prepared, maxWidth, lineHeight);
    return result.height + padding;
  } catch {
    // Fallback: rough estimate based on character count
    const charsPerLine = Math.max(1, Math.floor(maxWidth / 8));
    const lines = Math.ceil(text.length / charsPerLine);
    return lines * lineHeight + padding;
  }
}

// Cache for turn height estimates to avoid re-computation
const estimateCache = new Map<string, number>();
const MAX_CACHE_SIZE = 500;

function getCachedEstimate(key: string, computer: () => number): number {
  const cached = estimateCache.get(key);
  if (cached !== undefined) return cached;

  const value = computer();
  if (estimateCache.size >= MAX_CACHE_SIZE) {
    // Evict oldest entries
    const keys = estimateCache.keys();
    for (let i = 0; i < 100; i++) {
      const k = keys.next().value;
      if (k) estimateCache.delete(k);
    }
  }
  estimateCache.set(key, value);
  return value;
}

/**
 * Creates a virtualized scrolling setup with pretext height estimation.
 *
 * @param getScrollElement - Function that returns the scroll container element
 * @param count - Accessor that returns the total number of items
 * @param getTextForIndex - Function that returns the text content for a given index
 * @param options - Configuration options
 */
export function useVirtualScroll<T>(
  getScrollElement: () => HTMLElement | undefined,
  count: Accessor<number>,
  getTextForIndex: (index: number) => string = () => "",
  options: VirtualScrollOptions = {},
): VirtualScrollReturn<T> {
  const {
    overscan = 5,
    estimateSize = 200,
    gap = 0,
    containerWidth = 700,
    font = "14px system-ui",
    lineHeight = 22,
    turnPadding = 48,
  } = options;

  let virtualizer: Virtualizer<HTMLElement, T> | undefined;
  let scrollElement: HTMLElement | undefined;

  const totalSize = (): number => virtualizer?.getTotalSize() ?? 0;
  const virtualItems = (): ReturnType<Virtualizer<HTMLElement, T>["getVirtualItems"]> =>
    virtualizer?.getVirtualItems() ?? [];

  const getEstimate = (index: number): number => {
    const text = getTextForIndex(index);
    if (!text) return estimateSize;

    return getCachedEstimate(
      `turn-${index}-${text.length}-${text.slice(0, 50)}`,
      () => estimateTextHeight(text, font, containerWidth, lineHeight, turnPadding),
    );
  };

  const initVirtualizer = () => {
    scrollElement = getScrollElement();
    if (!scrollElement) return;

    // Update container width from actual element if available
    const actualWidth = scrollElement.clientWidth || containerWidth;

    virtualizer = createVirtualizer({
      get count() {
        return count();
      },
      getScrollElement: () => scrollElement,
      estimateSize: (index: number) => getEstimate(index),
      overscan,
      gap,
      measureElement: (element: HTMLElement | null) => {
        if (!element) return 0;
        return element.getBoundingClientRect().height;
      },
    });
  };

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
    getEstimate,
  };
}

/**
 * Clear the height estimation cache (call on session change).
 */
export function clearHeightCache(): void {
  estimateCache.clear();
  clearCache();
}
