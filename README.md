# Nexus Chat

> Discord-parity real-time chat built on [SpacetimeDB](https://spacetimedb.com). 40 tables, 95 reducers, agent-native, production-deployed.

<!-- Add hero gif here once captured: docs/images/demo.gif -->

**Try it live:** [spitfire-products.com](https://spitfire-products.com) — open the `CHAT` command from the command bar.

## What this is

This is the open-source reference implementation of the chat module powering [Nexus Terminal](https://spitfire-products.com), a real-time multi-module platform built on SpacetimeDB. It's extracted from a private monorepo and published as a standalone repo so the SpacetimeDB community has a working production-grade chat application to study, fork, and learn from.

What's in scope:

- **The Rust SpacetimeDB module** (40 tables, 95 reducers, 4 scheduled job types) — buildable as-is, deployable to your own SpacetimeDB instance
- **Architecture documentation** — how subscriptions are scoped, how the agent layer works, how the permission system maps to a Discord-style bitfield
- **Cross-references to [STDB Production Patterns](https://github.com/Spitfire-Products/stdb-cookbook)** — the patterns cookbook explains *why* this code is shaped the way it is

What's out of scope (for now):

- The full TypeScript/React client — the platform-side client is tightly coupled to Nexus Terminal's window manager, theme system, and module registry. A standalone reference client is on the roadmap (see [Status](#status))

## Stack

- **SpacetimeDB 2.0** with `features = ["unstable"]` (gates RLS compile-time macro)
- **Rust 1.93.0** (matches SpacetimeDB 2.0 MSRV)
- **TypeScript** for generated client bindings

## Highlights

Patterns and capabilities worth noting:

- **Real-time multi-user chat** — messages, reactions, edits, typing indicators, read receipts, threading, ephemeral messages, scheduled messages
- **Discord-style permission system** — 20 permission bits + ADMINISTRATOR override, role inheritance with channel overrides, member timeouts, auto-mod rules, audit log
- **Server/channel hierarchy** — chat servers (guilds) with categories, roles, custom emojis, stickers, invites, pinned messages, polls, forums, webhooks
- **Multi-device identity** — STDB identities mapped to application user IDs via `user_identity_links` with first-link-wins security and auto-prune. Same connection token works across reload, devices accumulate independently. Cookbook chapter: [Identity & Multi-Device Auth](https://github.com/Spitfire-Products/stdb-cookbook/blob/main/chapters/01-identity-and-multi-device-auth.md)
- **Bots as first-class users** — agents have their own STDB connections with own identities, heartbeat-driven presence, exponential-backoff reconnection, DDIL replay queues, and per-tier spawn quotas. Cookbook chapter: [Agent & Bot Architectures](https://github.com/Spitfire-Products/stdb-cookbook/blob/main/chapters/07-agent-and-bot-architectures.md)
- **Cross-module RPC** via authenticated HTTP-from-procedure calls — the agent runtime can post messages on behalf of bots without crossing the browser boundary. Cookbook chapter: [Cross-Module Communication](https://github.com/Spitfire-Products/stdb-cookbook/blob/main/chapters/02-cross-module-communication.md)
- **Shadow-index workaround** for the SpacetimeDB `Option<T>` filter limit — see `module/src/tables/reaction_room_index.rs` and the [Schema Discipline](https://github.com/Spitfire-Products/stdb-cookbook/blob/main/chapters/04-schema-discipline.md) cookbook chapter
- **Subscription scope discipline** — four-scope strategy (core / room / server / public-guest), wave-load deferred tables, lazy modal subscriptions, rAF-batched updates. Cookbook chapter: [Subscription Engineering](https://github.com/Spitfire-Products/stdb-cookbook/blob/main/chapters/03-subscription-engineering.md)

## Architecture

```
SpacetimeDB (wss://maincloud.spacetimedb.com, module: nexus-chat)
  ↕ WebSocket (human)                    ↕ WebSocket (per-agent)
ChatService                              AgentChatService (multi-instance)
  └─ Table listeners + 90 reducer wrappers  └─ Own identity, heartbeat, DDIL
  ↕                                          ↕
ChatProvider (React Context)                AgentChatManager (lifecycle orchestrator)
  ↕
27 UI components
```

**Subscription scopes:**

| Scope | Trigger | Tables |
|-------|---------|--------|
| Core | On authenticated connect | Users, rooms, servers, own memberships, drafts, read positions, bookmarks |
| Room | On room select | Messages, members, typing, reactions, edits, pins, attachments, polls |
| Server | On server select | Categories, roles, emojis, stickers, invites, auto-mod, timeouts, audit log |
| Public-guest | On unauthenticated connect | Servers, rooms, users, profiles (read-only) |

**Authentication:** `ctx.sender()` → `user_identity_links` table → application `user_id`. All write reducers call `get_caller_user_id()`. Identity links use first-link-wins security and auto-prune to 10 links per user.

**Permission system:** Discord-style `u64` bitfield. 20 permission bits + ADMINISTRATOR (bit 31). Resolution: @everyone base → OR all assigned role permissions → ADMINISTRATOR bypass → channel overrides (role-based, then member-specific).

For the deeper architecture walkthrough, see [`module/README.md`](module/README.md).

## Module overview

| | |
|---|---|
| Tables | 40 (37 public, 1 private — `webhooks`, `system_config`, `agent_credentials`) |
| Reducers | 95 across 12 files (auth, lifecycle, users, rooms, messages, typing, reactions, read tracking, scheduling, permissions, agent management, cross-module system) |
| Scheduled jobs | 4 (typing expiry, ephemeral cleanup, scheduled delivery, timeout expiry) |
| RLS filters | 1 (`user_identity_links` :sender-scoped) |
| MSRV | Rust 1.93.0 |

Full module reference: [`module/README.md`](module/README.md) (610 lines covering directory structure, every table, every reducer signature, scheduled job lifecycle, security model).

## Deploy your own

Prerequisites: [SpacetimeDB CLI](https://spacetimedb.com/install) v2.0+, Rust 1.93.0, `wasm32-unknown-unknown` target.

```bash
# Clone
git clone https://github.com/Spitfire-Products/nexus-chat
cd nexus-chat/module

# Build the WASM
cargo build --release --target wasm32-unknown-unknown

# Publish to SpacetimeDB (use your own module name, not "nexus-chat")
spacetime publish my-chat --bin-path target/wasm32-unknown-unknown/release/chat_spacetimedb.wasm

# Generate TypeScript bindings for your client
spacetime generate --lang typescript --out-dir generated --bin-path target/wasm32-unknown-unknown/release/chat_spacetimedb.wasm
```

For local development you can run SpacetimeDB locally; for production, point at `wss://maincloud.spacetimedb.com` (free tier). The module is identity-agnostic — it works with whatever upstream auth provider you wire your client to (Firebase, Auth0, Supabase, your own session token).

If you hit `cannot allocate memory in static TLS block` on Linux (notably Replit), set `GLIBC_TUNABLES=glibc.rtld.optional_static_tls=65536` before building.

## The cookbook

This repo is the working code; [STDB Production Patterns](https://github.com/Spitfire-Products/stdb-cookbook) is the prose explaining why every architectural decision is shaped the way it is. The two are companion artifacts — read whichever direction fits your style:

- **Reading the code first?** The cookbook chapters cite specific files and line numbers in this repo. Each pattern's "In the code" section points at the canonical implementation.
- **Reading the patterns first?** The cookbook chapters explain the problem each pattern solves before showing the code. Then come back here to see it in context.

The cookbook is also the staging ground for patterns we'd like to graduate into the [official SpacetimeDB skills](https://github.com/clockworklabs/SpacetimeDB/tree/master/skills) once the community has validated them.

## Status

This is a **reference implementation**, source-of-truth maintained in a private monorepo and synced to this repo on releases. Issues are welcome. PRs are reviewed best-effort — if you've spotted a bug or have a pattern to contribute, open one and we'll take a look.

**Roadmap:**

- [x] Module open-sourced (this repo)
- [ ] Standalone reference TS/React client — currently the client lives in the parent monorepo with platform dependencies. Decoupling into a self-contained Vite app is on the roadmap.
- [ ] Demo gif and screenshots — captured from the live deployment
- [ ] Discord bridge daemon — bidirectional Discord ↔ Nexus Chat relay (code complete, not yet tested live)

## License

[MIT](LICENSE) — use this code however you want, including in commercial projects. Attribution appreciated but not required.

## Links

- **Live deployment:** [spitfire-products.com](https://spitfire-products.com)
- **Patterns cookbook:** [github.com/Spitfire-Products/stdb-cookbook](https://github.com/Spitfire-Products/stdb-cookbook)
- **SpacetimeDB:** [spacetimedb.com](https://spacetimedb.com) · [GitHub](https://github.com/clockworklabs/SpacetimeDB) · [Discord](https://discord.gg/spacetimedb)
