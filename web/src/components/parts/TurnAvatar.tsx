import { type Component, Switch, Match } from "solid-js";

interface TurnAvatarProps {
  role: "user" | "assistant" | "system";
}

/**
 * Renders a small avatar for a conversational turn.
 * Uses Material Design 3 styling with the reference color palette.
 */
export const TurnAvatar: Component<TurnAvatarProps> = (props) => {
  return (
    <Switch>
      <Match when={props.role === "user"}>
        <div class="turn-avatar w-6 h-6 rounded-full flex-shrink-0 flex items-center justify-center" style={{ background: 'var(--avatar-user-bg)' }}>
          <span class="material-symbols-outlined text-sm" style={{ color: 'var(--avatar-user-color)' }}>person</span>
        </div>
      </Match>
      <Match when={props.role === "assistant"}>
        <div class="turn-avatar w-6 h-6 rounded-full flex-shrink-0 flex items-center justify-center" style={{ background: 'var(--avatar-assistant-bg)' }}>
          <span class="material-symbols-outlined text-sm" style={{ color: 'var(--avatar-assistant-color)', 'font-variation-settings': "'FILL' 1" }}>smart_toy</span>
        </div>
      </Match>
      <Match when={props.role === "system"}>
        <div class="turn-avatar w-6 h-6 rounded-full flex-shrink-0 flex items-center justify-center" style={{ background: 'var(--avatar-system-bg)' }}>
          <span class="material-symbols-outlined text-sm" style={{ color: 'var(--avatar-system-color)' }}>info</span>
        </div>
      </Match>
    </Switch>
  );
};
