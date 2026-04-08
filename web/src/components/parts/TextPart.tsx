import { type Component } from "solid-js";
import { MarkdownRenderer } from "../MarkdownRenderer";

interface TextPartProps {
  content: string;
}

export const TextPart: Component<TextPartProps> = (props) => {
  return (
    <div data-part="text" class="text-part">
      <MarkdownRenderer content={props.content} />
    </div>
  );
};
