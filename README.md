# Plasma

[![License: MPL-2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)
[![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-blue.svg)](https://tauri.app/)

A local-first, AI-powered tool for building iOS and Android apps.

## Motivation

Platform-native editors like [Xcode](https://developer.apple.com/xcode/) and [Android Studio](https://developer.android.com/studio) are adding AI features incrementally, but they're constrained by decades of existing UI and workflows. There's an opportunity to rethink app development from scratch, where traditional IDE features and direct code manipulation become optional rather than central.

Inspired by [Tidewave](https://tidewave.ai/).

## Development

Install dependencies with [mise](https://mise.jdx.dev/):

```bash
mise install
```

Run the desktop app:

```bash
cd app && cargo tauri dev
```

Run the frontend (for hot reload):

```bash
cd frontend && pnpm dev
```

## License

MPL-2.0
