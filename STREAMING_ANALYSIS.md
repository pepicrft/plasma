# Simulator Streaming Architecture Analysis

## Problem
Current implementation freezes intermittently despite backend streaming 60 FPS. The issue is architectural, not browser-based.

## Current Architecture (Plasma)
```
Request → plasma-stream (spawned per-request)
  ↓
  Spawns new process
  ↓
  Extracts IOSurface
  ↓
  Encodes MJPEG to stdout
  ↓
  Rust proxy reads chunks
  ↓
  HTTP response (multipart/x-mixed-replace)
  ↓
  Browser <img> tag consumes
```

### Problems:
1. **New process per stream request** - Each refresh/reconnect spawns fresh binary
2. **No state persistence** - IOSurface callbacks are lost on process restart
3. **No interaction channel** - Can't send commands (rotations, touches) through same connection
4. **Buffering issues** - Large chunks (65KB) can cause frame drops
5. **No back-pressure handling** - Can't tell if client is connected or consuming frames

## radon-ide Architecture
```
Single long-lived process (simulator-server)
  ↓
  Persistent stdin/stdout communication
  ↓
  Register IOSurface callbacks (stays alive)
  ↓
  Serves HTTP stream on internal port
  ↓
  Handles interactive commands via stdin pipe
    - rotate
    - touch (with coordinate transformation)
    - button presses
    - keyboard input
    - screen recording
    - FPS reporting
```

### Advantages:
1. **Single persistent connection** - One binary stays running
2. **State is preserved** - IOSurface callbacks never reset
3. **Bi-directional communication** - Commands + streaming over same connection
4. **Built-in FPS reporting** - Detects lag/drops
5. **Interactive features** - Touches, rotations, buttons all supported

## Solution: Implement a Plasma Simulator Server

Create a unified `simulator-server` binary (Swift) that:

1. **Maintains persistent connection to simulator** via CoreSimulator APIs
2. **Registers IOSurface callbacks** that stay alive
3. **Exposes HTTP endpoint for MJPEG** on localhost:PORT
4. **Opens stdin pipe for commands** (rotations, touches, etc.)
5. **Reports FPS metrics** via stdout
6. **Handles graceful shutdown** via stdin

### Implementation Steps:

1. **Create Tools/simulator-server (Swift package)**
   - Similar to plasma-stream but adds HTTP server + stdin handling
   - Register persistent IOSurface callbacks
   - Keep binary running, not spawning per-request

2. **Update Backend** (Rust)
   - Spawn simulator-server once (per simulator session)
   - Keep it running in background
   - Read its stdout for stream URL and metrics
   - Send commands via stdin

3. **Update Frontend**
   - Simple `<img>` tag (what you have now works)
   - Add WebSocket/HTTP endpoints to send commands
   - Listen for FPS metrics

## Why Current Freezes

**Root Cause**: Each HTTP request creates new `plasma-stream` process:
1. IOSurface callbacks initialize
2. First few frames work  
3. Callbacks may miss updates while browser reconnects
4. Process killed when stream ends
5. Restart loses state

**radon-ide avoids this** by keeping ONE process alive that continuously encodes frames.

## Quick Win (Immediate)
Modify your Rust backend to:
1. Spawn `plasma-stream` ONCE and keep it running
2. Don't kill it when clients disconnect
3. Cache its port/stdout URL
4. Clients reconnect to same stream

This is 80% of the fix without rewriting everything.

## Long-term (Right Solution)
Build unified `simulator-server` like radon-ide that handles:
- Streaming
- Interactions (rotations, touches)
- Metrics
- Recording
- Screenshots
