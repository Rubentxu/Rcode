import { type Component, createMemo, createSignal, createEffect, For, Show, onCleanup } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface StreamingTextPartProps {
  content: string;
  throttleMs?: number;
}

/**
 * Splits content into markdown blocks by double newlines.
 * Used for memoized block-level rendering during streaming.
 */
function splitMarkdownBlocks(content: string): string[] {
  if (!content) return [];
  return content.split(/(?:\r?\n){2,}/);
}

/**
 * Checks if content ends with an unclosed code fence.
 */
function hasUnclosedCodeFence(content: string): boolean {
  const fenceRegex = /```[\w]*$/;
  return fenceRegex.test(content);
}

export const StreamingTextPart: Component<StreamingTextPartProps> = (props) => {
  const throttleMs = props.throttleMs ?? 100;
  
  // Track the last content for throttle
  const [throttledContent, setThrottledContent] = createSignal(props.content);
  let lastUpdateTime = 0;
  let scheduledUpdate: ReturnType<typeof setTimeout> | null = null;
  
  // Split into blocks
  const blocks = createMemo(() => splitMarkdownBlocks(throttledContent()));
  
  // Check for incomplete code fence in the last block
  const hasIncompleteFence = createMemo(() => {
    const content = throttledContent();
    return hasUnclosedCodeFence(content);
  });
  
  // Memoize all but the last block
  const completedBlocks = createMemo(() => {
    const allBlocks = blocks();
    if (allBlocks.length <= 1) return [];
    return allBlocks.slice(0, -1);
  });
  
  // Active block (last one) - re-renders on updates
  const activeBlock = createMemo(() => {
    const allBlocks = blocks();
    return allBlocks[allBlocks.length - 1] ?? "";
  });
  
  // Update with throttle - using createEffect to react to prop changes
  createEffect(() => {
    const newContent = props.content;
    
    const updateContent = (content: string) => {
      const now = Date.now();
      const timeSinceLastUpdate = now - lastUpdateTime;
      
      if (timeSinceLastUpdate >= throttleMs) {
        lastUpdateTime = now;
        setThrottledContent(content);
      } else {
        // Schedule update for later
        if (scheduledUpdate) {
          clearTimeout(scheduledUpdate);
        }
        scheduledUpdate = setTimeout(() => {
          lastUpdateTime = Date.now();
          setThrottledContent(content);
          scheduledUpdate = null;
        }, throttleMs - timeSinceLastUpdate);
      }
    };
    
    updateContent(newContent);
  });
  
  // Cleanup scheduled update on unmount
  onCleanup(() => {
    if (scheduledUpdate) {
      clearTimeout(scheduledUpdate);
      scheduledUpdate = null;
    }
  });
  
  return (
    <div data-component="streaming-text-part" class="text-part">
      {/* Completed blocks are memoized */}
      <For each={completedBlocks()}>
        {(block) => (
          <div data-component="completed-block" class="completed-block">
            <MarkdownRenderer content={block} />
          </div>
        )}
      </For>
      
      {/* Active block re-renders on each delta */}
      <div data-component="active-block" class="active-block">
        <Show 
          when={!hasIncompleteFence()} 
          fallback={<pre class="incomplete-code">{activeBlock()}</pre>}
        >
          <MarkdownRenderer content={activeBlock()} />
        </Show>
      </div>
    </div>
  );
};
