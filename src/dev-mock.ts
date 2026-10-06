// Used only by `npm run dev` in a plain browser (no Tauri), so the UI can be
// previewed and tested without building the Rust app. Never part of a release
// build: main.ts imports this behind `import.meta.env.DEV`.
import { mockIPC } from "@tauri-apps/api/mocks";

export function installMock() {
  let settings = {
    provider: "anthropic",
    model: "claude-haiku-4-5-20251001",
    base_url: null as string | null,
    restore_clipboard: false,
    presets: [
      {
        id: "grammar",
        name: "Fix grammar",
        hotkey: "CmdOrCtrl+Shift+G",
        prompt: "Fix the grammar, spelling and punctuation of the text.",
      },
    ],
  };
  const keys = new Set<string>();

  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, any>;
      switch (cmd) {
        case "get_settings":
          return structuredClone(settings);
        case "save_settings": {
          const seen = new Set<string>();
          for (const p of a.settings.presets) {
            if (seen.has(p.hotkey)) throw `Could not register "${p.hotkey}": already registered`;
            seen.add(p.hotkey);
          }
          settings = a.settings;
          return null;
        }
        case "set_api_key":
          keys.add(a.provider);
          return null;
        case "has_api_key":
          return keys.has(a.provider);
        case "default_models":
          return [
            ["anthropic", "claude-haiku-4-5-20251001"],
            ["openai", "gpt-4o-mini"],
            ["gemini", "gemini-2.5-flash"],
          ];
        case "try_fix":
          if (!keys.has(settings.provider)) throw `No API key saved for ${settings.provider}`;
          return new Promise((r) => setTimeout(() => r(`(mock) ${a.text}`), 600));
        case "pause_hotkeys":
        case "resume_hotkeys":
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
