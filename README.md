# Fixit

A small menu-bar app that fixes grammar with your own AI API key. Select text in any app, press a hotkey, and the selection is replaced with the corrected version. The result is also left on your clipboard.

- Works with **Anthropic (Claude)**, **OpenAI (ChatGPT)** and **Google (Gemini)**, plus any OpenAI-compatible server (Ollama, OpenRouter, Groq, ...) through a custom base URL.
- Fully customisable prompts. Make as many presets as you like (fix grammar, make formal, translate, shorten...), each with its own hotkey.
- English and Arabic (and any other language: the model keeps the language of your text). Right-to-left text displays correctly.
- No server and no account. Your text goes straight from your machine to the provider you chose, and your API key is stored in the OS keychain (macOS Keychain / Windows Credential Manager).

## How it works

1. You press the hotkey.
2. Fixit copies your selection (it sends Cmd/Ctrl+C for you).
3. It sends the text to your provider with your prompt.
4. It pastes the result over the selection (Cmd/Ctrl+V) and leaves it on the clipboard. Cmd/Ctrl+Z undoes it.

If nothing is selected, Fixit fixes the text already on your clipboard and only copies the result.

The tray icon shows progress (and errors) while the settings window is closed.

## Install

Download the installer for your system from the [Releases](../../releases) page.

### macOS

Fixit needs **Accessibility** permission so it can copy and paste for you:
*System Settings > Privacy & Security > Accessibility*, then switch Fixit on.

The builds are not notarized, so macOS may say the app is damaged or from an unidentified developer. Right-click the app and choose **Open**, or run:

```bash
xattr -cr /Applications/Fixit.app
```

### Windows

The installer is not code-signed, so SmartScreen may warn you. Choose **More info > Run anyway**. Fixit can't paste into windows running as administrator.

## Setup

1. Open **Settings** from the tray icon.
2. Choose a provider and paste your API key.
3. Press **Try it** to check the key works.
4. Select some text anywhere and press the hotkey (default `Cmd/Ctrl+Shift+G`).

## Build from source

You need [Node.js](https://nodejs.org) and [Rust](https://rustup.rs).

```bash
npm install
npm run tauri dev     # run in development
npm run tauri build   # produce an installer for your OS
```

Run the tests with `cargo test` inside `src-tauri/`.

## Privacy

Fixit has no backend and sends nothing anywhere except your chosen AI provider. The text you fix is sent to that provider, so their terms and pricing apply.

## Known limits

- Replacing text goes through the clipboard, so rich formatting (bold, links) in the selection is lost.
- While fixing, the clipboard is used temporarily; copied images or files aren't restored. There's an option to restore your previous clipboard *text*.
- Password fields and apps that block copy and paste can't be fixed.
- Linux isn't supported yet.
