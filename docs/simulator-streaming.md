# Simulator Streaming (Final System)

This document describes the final simulator streaming path used by Plasma and
why it is smooth.

## High-level flow

1) Launch simulator and app via Xcode build/run.
2) Start capture in this order:
   - fbsimctl (raw BGRA stream)
   - simulator-server (MJPEG)
   - window capture
   - simctl screenshots
3) Render latest frame in the UI loop at ~60 Hz (poll + drop old frames).

## Primary stream: fbsimctl (BGRA)

We use the FBSimulatorControl CLI (`fbsimctl`) to stream the simulator display.
It provides a raw BGRA stream plus a one-time attributes block:

`Mounting Surface with Attributes: {width = ...; height = ...; row_size = ...; frame_size = ...}`

Key implementation details:
- We parse the attributes block across multiple lines and handle quoted keys.
- `row_size` can be larger than `width * 4`, so we strip per-row padding.
- We keep the data in BGRA because `RenderImage` expects BGRA; no per-pixel
  channel swap is needed.

This avoids expensive conversions and keeps capture latency low.

## Frame pipeline

- Capture thread reads from fbsimctl stdout.
- Frame bytes are assembled using `frame_size` and `row_size`.
- We build `RenderImage` directly from the BGRA buffer.
- Frames are sent via a `sync_channel(1)` with `try_send` so old frames are
  dropped when the UI is busy.
- UI loop drains with `try_iter` and applies the latest frame only.

## Environment knobs

- `PLASMA_FBSIMCTL=/path/to/fbsimctl`
  Override the fbsimctl binary path.
- `PLASMA_FBSIMCTL_FPS=30`
  Capture FPS for fbsimctl.
- `PLASMA_FBSIMCTL_DEBUG=1`
  Enable `--debug-logging` for fbsimctl diagnostics.

## Why this is smooth

- Zero unnecessary color conversion (BGRA passthrough).
- Padding-aware row copy (avoids per-pixel loops).
- Latest-only frame propagation (no backlog).
- Uses IOSurface-backed simulator stream via FBSimulatorControl.

If the Simulator UI itself is slow, the stream will reflect that. Plasma will not
display frames faster than the Simulator can render them.
