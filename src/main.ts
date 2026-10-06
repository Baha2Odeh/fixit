import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { captureFromEvent, displayHotkey } from "./hotkey";

type Provider = "anthropic" | "openai" | "gemini";

interface Preset {
  id: string;
  name: string;
  hotkey: string;
  prompt: string;
}

interface Settings {
  provider: Provider;
  model: string;
  base_url: string | null;
  restore_clipboard: boolean;
  presets: Preset[];
}

type Status =
  | { kind: "working" }
  | { kind: "done"; pasted: boolean }
  | { kind: "error"; message: string };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const els = {
  provider: $<HTMLSelectElement>("provider"),
  model: $<HTMLInputElement>("model"),
  apiKey: $<HTMLInputElement>("api-key"),
  saveKey: $<HTMLButtonElement>("save-key"),
  keyHint: $("key-hint"),
  baseUrl: $<HTMLInputElement>("base-url"),
  restore: $<HTMLInputElement>("restore-clipboard"),
  presets: $("presets"),
  addPreset: $<HTMLButtonElement>("add-preset"),
  tryPreset: $<HTMLSelectElement>("try-preset"),
  tryInput: $<HTMLTextAreaElement>("try-input"),
  tryRun: $<HTMLButtonElement>("try-run"),
  tryState: $("try-state"),
  tryOutput: $("try-output"),
  save: $<HTMLButtonElement>("save"),
  saveMsg: $("save-msg"),
  status: $("status"),
};

let settings: Settings;
let defaults = new Map<Provider, string>();

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

/** The Save button is enabled exactly while there are unsaved changes. */
function setDirty(value: boolean) {
  els.save.disabled = !value;
  if (value) flash(els.saveMsg, "Unsaved changes");
}

function flash(el: HTMLElement, text: string, kind: "ok" | "err" | "" = "") {
  el.textContent = text;
  el.className = kind;
}

// ---------- provider section ----------

async function refreshKeyHint() {
  const provider = els.provider.value as Provider;
  try {
    const has = await invoke<boolean>("has_api_key", { provider });
    els.apiKey.placeholder = has ? "Saved in your keychain. Type to replace it" : "Paste your API key";
    flash(els.keyHint, has ? "A key is saved for this provider." : "No key saved for this provider yet.", has ? "ok" : "");
  } catch (e) {
    flash(els.keyHint, errText(e), "err");
  }
}

function renderProvider() {
  els.provider.value = settings.provider;
  els.model.value = settings.model;
  els.model.placeholder = defaults.get(settings.provider) ?? "";
  els.baseUrl.value = settings.base_url ?? "";
  els.restore.checked = settings.restore_clipboard;
  refreshKeyHint();
}

els.provider.addEventListener("change", () => {
  const previous = settings.provider;
  const next = els.provider.value as Provider;
  // Swap the model only if the user never customised it.
  if (!els.model.value.trim() || els.model.value === defaults.get(previous)) {
    els.model.value = defaults.get(next) ?? "";
  }
  els.model.placeholder = defaults.get(next) ?? "";
  settings.provider = next;
  settings.model = els.model.value;
  setDirty(true);
  refreshKeyHint();
});

els.model.addEventListener("input", () => {
  settings.model = els.model.value;
  setDirty(true);
});
els.baseUrl.addEventListener("input", () => {
  settings.base_url = els.baseUrl.value.trim() || null;
  setDirty(true);
});
els.restore.addEventListener("change", () => {
  settings.restore_clipboard = els.restore.checked;
  setDirty(true);
});

async function saveKey() {
  const key = els.apiKey.value.trim();
  if (!key) return;
  try {
    // The key goes straight to the OS keychain. It is never stored in settings.json.
    await invoke("set_api_key", { provider: els.provider.value, key });
    els.apiKey.value = "";
    await refreshKeyHint();
    flash(els.keyHint, "Key saved to your keychain.", "ok");
  } catch (e) {
    flash(els.keyHint, `Could not save the key: ${errText(e)}`, "err");
  }
}
els.saveKey.addEventListener("click", saveKey);
els.apiKey.addEventListener("keydown", (e) => {
  if (e.key === "Enter") saveKey();
});

// ---------- presets ----------

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { class?: string } = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) node.className = cls;
  Object.assign(node, rest);
  node.append(...children);
  return node;
}

function renderTryPresets() {
  const current = els.tryPreset.value;
  els.tryPreset.replaceChildren(
    ...settings.presets.map((p) => el("option", { value: p.id, textContent: p.name || "(unnamed)" })),
  );
  if (settings.presets.some((p) => p.id === current)) els.tryPreset.value = current;
}

/** Ends the recording in progress, if any. Only one preset records at a time. */
let stopCapture: (() => void) | null = null;

function startCapture(button: HTMLButtonElement, preset: Preset, hint: HTMLElement) {
  // Clicking the button that is already recording cancels it.
  const wasRecordingThis = button.classList.contains("recording");
  stopCapture?.();
  if (wasRecordingThis) return;

  button.textContent = "Press a shortcut…";
  button.classList.add("recording");
  hint.textContent = "Esc to cancel";
  // Live hotkeys are paused so pressing an existing shortcut can't trigger the flow.
  invoke("pause_hotkeys").catch(() => {});

  // Listen on the window, not the button: WebKit (macOS) doesn't focus a button
  // when it is clicked, so keydown would never reach a listener on the button.
  const finish = () => {
    window.removeEventListener("keydown", onKey, true);
    window.removeEventListener("pointerdown", onOutsideClick, true);
    stopCapture = null;
    button.classList.remove("recording");
    button.textContent = displayHotkey(preset.hotkey) || "Click to set";
    hint.textContent = "";
    invoke("resume_hotkeys").catch(() => {});
  };
  const onOutsideClick = (e: PointerEvent) => {
    if (e.target !== button) finish();
  };
  const onKey = (e: KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return finish();
    const result = captureFromEvent(e);
    if (result.kind === "pending") return;
    if (result.kind === "invalid") {
      hint.textContent = result.reason;
      return;
    }
    preset.hotkey = result.hotkey;
    setDirty(true);
    finish();
  };
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("pointerdown", onOutsideClick, true);
  stopCapture = finish;
}

function renderPresets() {
  els.presets.replaceChildren(
    ...settings.presets.map((preset) => {
      const name = el("input", { value: preset.name, placeholder: "Preset name", spellcheck: false });
      name.addEventListener("input", () => {
        preset.name = name.value;
        renderTryPresets();
        setDirty(true);
      });

      const hint = el("small", { class: "hotkey-hint" });
      const hotkey = el("button", { type: "button", class: "hotkey", textContent: displayHotkey(preset.hotkey) || "Click to set" });
      hotkey.addEventListener("click", () => startCapture(hotkey, preset, hint));

      const remove = el("button", { type: "button", class: "danger", textContent: "Delete" });
      remove.disabled = settings.presets.length === 1;
      remove.title = remove.disabled ? "At least one preset is required" : "Delete this preset";
      remove.addEventListener("click", () => {
        settings.presets = settings.presets.filter((p) => p.id !== preset.id);
        renderPresets();
        setDirty(true);
      });

      // dir="auto" so Arabic prompts and text render right-to-left.
      const prompt = el("textarea", { rows: 4, value: preset.prompt, dir: "auto", placeholder: "Instructions for the model" });
      prompt.addEventListener("input", () => {
        preset.prompt = prompt.value;
        setDirty(true);
      });

      return el(
        "div",
        { class: "preset" },
        el("div", { class: "preset-head" }, name, hotkey, remove),
        hint,
        prompt,
      );
    }),
  );
  renderTryPresets();
}

els.addPreset.addEventListener("click", () => {
  settings.presets.push({
    id: crypto.randomUUID(),
    name: "New preset",
    hotkey: "",
    prompt: "Rewrite the text more clearly and concisely. Keep its meaning.",
  });
  renderPresets();
  setDirty(true);
});

// ---------- save ----------

function validate(): string | null {
  if (!els.model.value.trim()) return "Model can't be empty.";
  const seen = new Set<string>();
  for (const p of settings.presets) {
    const label = p.name.trim() || "A preset";
    if (!p.name.trim()) return "Every preset needs a name.";
    if (!p.prompt.trim()) return `"${label}" needs a prompt.`;
    if (!p.hotkey) return `"${label}" needs a hotkey.`;
    if (seen.has(p.hotkey)) return `Two presets use the same hotkey (${displayHotkey(p.hotkey)}).`;
    seen.add(p.hotkey);
  }
  return null;
}

els.save.addEventListener("click", async () => {
  const problem = validate();
  if (problem) return flash(els.saveMsg, problem, "err");
  els.save.disabled = true;
  try {
    // The Rust side registers every hotkey first, so a taken shortcut is reported
    // here and nothing is saved.
    await invoke("save_settings", { settings });
    flash(els.saveMsg, "Saved. Hotkeys are active.", "ok");
  } catch (e) {
    els.save.disabled = false;
    flash(els.saveMsg, errText(e), "err");
  }
});

// ---------- try it ----------

els.tryRun.addEventListener("click", async () => {
  const preset = settings.presets.find((p) => p.id === els.tryPreset.value);
  const text = els.tryInput.value;
  if (!preset || !text.trim()) return;
  els.tryRun.disabled = true;
  els.tryOutput.hidden = true;
  flash(els.tryState, "Working…");
  try {
    const out = await invoke<string>("try_fix", { prompt: preset.prompt, text });
    els.tryOutput.textContent = out;
    els.tryOutput.className = "";
    els.tryOutput.hidden = false;
    flash(els.tryState, "");
  } catch (e) {
    els.tryOutput.textContent = errText(e);
    els.tryOutput.className = "err";
    els.tryOutput.hidden = false;
    flash(els.tryState, "");
  } finally {
    els.tryRun.disabled = false;
  }
});

// ---------- live status from the hotkey flow ----------

let statusTimer: number | undefined;

function showStatus(s: Status) {
  window.clearTimeout(statusTimer);
  els.status.className = `status ${s.kind}`;
  if (s.kind === "working") els.status.textContent = "Working…";
  else if (s.kind === "done") {
    els.status.textContent = s.pasted ? "Replaced" : "Copied to clipboard";
    statusTimer = window.setTimeout(() => ((els.status.textContent = ""), (els.status.className = "status")), 4000);
  } else els.status.textContent = s.message;
}

// ---------- boot ----------

async function boot() {
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installMock } = await import("./dev-mock");
    installMock();
  }
  await listen<Status>("fixit:status", (e) => showStatus(e.payload));
  try {
    const [loaded, models] = await Promise.all([
      invoke<Settings>("get_settings"),
      invoke<[Provider, string][]>("default_models"),
    ]);
    settings = loaded;
    defaults = new Map(models);
    renderProvider();
    renderPresets();
  } catch (e) {
    flash(els.saveMsg, `Could not load settings: ${errText(e)}`, "err");
  }
}

boot();
