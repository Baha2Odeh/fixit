// Converts keyboard events into the shortcut strings the Rust side parses
// (e.g. "CmdOrCtrl+Shift+G") and formats them for display.

const isMac = navigator.userAgent.includes("Mac");

const MODIFIER_KEYS = new Set(["Meta", "Control", "Alt", "Shift"]);

// Only keys we know the Rust parser accepts. Punctuation is left out on purpose.
const NAMED_KEYS = new Set([
  "Space", "Enter", "Home", "End", "PageUp", "PageDown",
  "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
]);

function keyName(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit(\d)$/.exec(code);
  if (digit) return digit[1];
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return NAMED_KEYS.has(code) ? code : null;
}

export type Captured =
  | { kind: "pending" } // only modifiers held so far
  | { kind: "invalid"; reason: string }
  | { kind: "ok"; hotkey: string };

export function captureFromEvent(e: KeyboardEvent): Captured {
  if (MODIFIER_KEYS.has(e.key)) return { kind: "pending" };

  const mods: string[] = [];
  if (isMac) {
    if (e.metaKey) mods.push("CmdOrCtrl");
    if (e.ctrlKey) mods.push("Ctrl");
  } else {
    if (e.ctrlKey) mods.push("CmdOrCtrl");
    if (e.metaKey) mods.push("Super");
  }
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");

  // A global hotkey without a modifier would hijack normal typing.
  if (mods.length === 0) {
    return { kind: "invalid", reason: "Include at least one modifier (Cmd/Ctrl, Alt or Shift)." };
  }
  const key = keyName(e.code);
  if (!key) {
    return { kind: "invalid", reason: "Use a letter, number, F-key, arrow, Space or Enter." };
  }
  return { kind: "ok", hotkey: [...mods, key].join("+") };
}

const MAC_SYMBOLS: Record<string, string> = {
  CmdOrCtrl: "⌘", Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘",
};

export function displayHotkey(hotkey: string): string {
  const parts = hotkey.split("+");
  if (isMac) {
    return parts.map((p) => MAC_SYMBOLS[p] ?? p.replace(/^Arrow/, "")).join(" ");
  }
  return parts.map((p) => (p === "CmdOrCtrl" ? "Ctrl" : p.replace(/^Arrow/, ""))).join(" + ");
}
