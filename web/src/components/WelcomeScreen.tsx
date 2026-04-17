interface WelcomeScreenProps {
  onAddProject: () => void;
}

export default function WelcomeScreen(props: WelcomeScreenProps) {
  return (
    <div
      data-component="welcome-screen"
      class="flex flex-col items-center justify-center h-full px-6 py-12 gap-8"
    >
      {/* Brand hero */}
      <div class="flex flex-col items-center gap-4">
        <div
          class="w-16 h-16 rounded-2xl flex items-center justify-center"
          style="background: radial-gradient(circle at 30% 30%, var(--accent-bg-hover) 0%, var(--accent-bg-subtle) 100%); border: 1px solid var(--accent-border-subtle);"
        >
          <svg
            width="32"
            height="32"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            style="color: var(--accent);"
          >
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          </svg>
        </div>
        <div class="text-center">
          <h1
            class="text-2xl font-bold mb-1"
            style="background: var(--brand-gradient); -webkit-background-clip: text; -webkit-text-fill-color: transparent;"
          >
            RCode
          </h1>
          <p class="text-sm text-outline">
            Your AI-powered coding assistant
          </p>
        </div>
      </div>

      {/* Tagline */}
      <div class="text-center max-w-md">
        <p class="text-sm text-on-surface-variant">
          Open a project folder to get started with intelligent code assistance.
        </p>
      </div>

      {/* Primary CTA */}
      <button
        data-component="button"
        data-variant="primary"
        onClick={props.onAddProject}
        class="flex items-center gap-2 px-6 py-3 rounded-lg font-semibold text-sm transition-all duration-200 hover:opacity-90 active:scale-95"
        style="background: var(--primary); color: var(--on-primary);"
      >
        <span class="material-symbols-outlined text-lg">folder_open</span>
        Open Folder
      </button>

      {/* Keyboard hints */}
      <div class="flex items-center gap-3 flex-wrap justify-center">
        <span class="text-xs text-outline opacity-50">
          Add a project workspace to begin
        </span>
      </div>
    </div>
  );
}
