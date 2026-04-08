import { type Component } from "solid-js";

interface AttachmentPartProps {
  id: string;
  name: string;
  mime_type: string;
  content: unknown;
}

export const AttachmentPart: Component<AttachmentPartProps> = (props) => {
  // Get file size if content is available
  const getSize = () => {
    if (!props.content) return null;
    if (typeof props.content === "string") {
      // Base64 encoded
      const bytes = atob(props.content).length;
      return formatBytes(bytes);
    }
    return null;
  };

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  };

  // Determine file icon based on mime type
  const getFileIcon = () => {
    if (props.mime_type.startsWith("image/")) return "🖼️";
    if (props.mime_type.startsWith("audio/")) return "🎵";
    if (props.mime_type.startsWith("video/")) return "🎬";
    if (props.mime_type.includes("pdf")) return "📄";
    if (props.mime_type.includes("text")) return "📝";
    return "📎";
  };

  return (
    <div data-part="attachment" class="attachment-part">
      <div class="attachment-card">
        <span class="attachment-icon">{getFileIcon()}</span>
        <div class="attachment-info">
          <span class="attachment-name">{props.name}</span>
          <span class="attachment-meta">
            {props.mime_type}
            {getSize() && ` • ${getSize()}`}
          </span>
        </div>
        <button 
          class="attachment-download"
          onClick={() => {
            // Download trigger - would connect to actual download logic
            console.log("Download attachment:", props.id, props.name);
          }}
          title="Download"
        >
          ⬇
        </button>
      </div>
    </div>
  );
};
