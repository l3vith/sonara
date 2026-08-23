# Sonora

Sonora is a cross-platform desktop app for sharing audio in private, peer-to-peer listening rooms.

## Development

Requirements: Node.js, Rust, and platform-native build tools. On macOS, Screen Recording permission is required to capture app or system audio.

```sh
npm ci
npm run tauri dev
```

## Build

```sh
npm run tauri build
```

The generated installer is placed under `src-tauri/target/release/bundle/`.
