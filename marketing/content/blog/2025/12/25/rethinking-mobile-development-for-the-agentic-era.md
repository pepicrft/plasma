+++
title = "Rethinking mobile development for the agentic era"
description = "Why mobile IDEs need to evolve beyond traditional paradigms to embrace AI agents as first-class citizens in the development experience."
+++

Coding agents are quickly becoming the standard way developers interact with AI. Tools like [Claude Code](https://docs.anthropic.com/en/docs/claude-code), [Cursor](https://cursor.com), and [Windsurf](https://windsurf.com) have shown that the future of programming is conversational: you describe what you want, and an agent figures out how to make it happen. This shift is not a fad. It is a fundamental change in how we think about writing software.

But there is a gap. While web and backend development have embraced this agentic approach, mobile development is still stuck in the past. [Xcode](https://developer.apple.com/xcode/) and [Android Studio](https://developer.android.com/studio) are incredible tools, but they were designed for a world where developers write every line of code themselves. They optimize for manual coding: syntax highlighting, autocompletion, code navigation. These features matter, but they are not what matters most when an AI agent is doing the heavy lifting.

## The opportunity

What if we started from scratch? What if we designed a mobile development environment where chat and agents are not bolted on as an afterthought, but are the core of the experience?

This is the question that led us to build Plasma.

We took inspiration from [Tidewave](https://tidewave.dev), which reimagined web development with agents at the center. Tidewave showed that when you design for agents first, everything changes. The IDE becomes a collaborator, not just a text editor. Context flows naturally to the agent. Actions are exposed as tools the agent can use. The developer becomes an orchestrator rather than a typist.

We want to bring that same philosophy to mobile.

## Beyond cross-platform abstractions

For years, the mobile industry's answer to "build once, run everywhere" has been cross-platform frameworks like [React Native](https://reactnative.dev) and [Flutter](https://flutter.dev). These abstractions let teams move fast, but they come at a cost: performance trade-offs, platform-specific edge cases, and a layer of indirection between you and the native APIs.

Agents change this equation entirely. When an AI can write native code for each platform, you no longer need to abstract away the differences. You can have the best of both worlds: native performance and APIs on each platform, with a single conversation driving development for all of them.

OpenAI shared [how they built the Sora iOS app](https://openai.com/index/how-we-built-the-sora-ios-app/) in record time using agentic coding. The takeaway is clear: when agents handle the implementation, building for multiple platforms stops being a multiplication of effort. You describe the feature once, and the agent writes it for iOS, Android, or both.

This is the direction we are exploring with Plasma. Not another abstraction layer, but a new way of working where you can build native apps for multiple platforms simultaneously.

## What this means in practice

In Plasma, chat is front and center. You describe what you want to build, and AI agents have the context and tools they need to make it happen. Want to add a new screen? Describe it. Need to fix a bug? Point the agent at it. The traditional IDE features are still there if you need them, but they are optional. The primary interface is conversation.

At its core, Plasma is about bridging AI agents with mobile build toolchains and runtime capabilities. Agents are powerful, but they need the right context and tools to be effective. Plasma exposes project structure, build systems, simulators, and device logs in ways that agents can understand and act on. [José Valim](https://github.com/josevalim) calls this "runtime intelligence," and he is onto something with that idea. This connection between agent capabilities and platform tooling is what enables new coding experiences that were not possible before.

This is not about replacing developers. It is about amplifying them. A single developer with the right tools can build what used to require a team. That is the promise of agentic coding, and mobile developers deserve access to it.

## Early days

Plasma is in early development. We are exploring what a truly agent-first mobile IDE looks like. Some ideas will work, others will not. But we believe this direction is worth pursuing.

This is one of several explorations I am doing around future directions for [Tuist](https://tuist.dev). Tuist has always been about making mobile development better, and understanding how agents will reshape our workflows feels essential to that mission. Plasma is a way to learn by building.

If you are interested in following along or contributing, check out the project on [GitHub](https://github.com/pepicrft/Plasma). We are building in the open because we think the best ideas come from collaboration.

The future of mobile development is agentic. Let's build it together.
