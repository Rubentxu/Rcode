import { type Component } from "solid-js";

interface ToolCallCardProps {
  id: string;
  name: string;
  arguments: unknown;
}

export const ToolCallCard: Component<ToolCallCardProps> = (props) => {
  const formattedArgs = () => {
    try {
      return JSON.stringify(props.arguments, null, 2);
    } catch {
      return String(props.arguments);
    }
  };

  return (
    <div data-part="tool_call" class="tool-call-card">
      <div class="tool-call-header">
        <span class="tool-call-icon">⚡</span>
        <span class="tool-call-name">{props.name}</span>
      </div>
      <div class="tool-call-args">
        <pre><code>{formattedArgs()}</code></pre>
      </div>
    </div>
  );
};
