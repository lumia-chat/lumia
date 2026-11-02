# Lumia

Lumia is a chat app you can host yourself. Servers with public and private channels, direct messages inside and outside servers, threads, reactions, and calls, in the shape you know from Discord and Slack. The difference is who runs it: anyone can host an instance, an instance can hold many servers, and one client connects to as many instances as you like, as one flat list of servers.

Personal DMs are end-to-end encrypted. Server content is readable by the people who run the server, and the app tells you which is which.

## Status

Design stage. The architecture and protocol are written; the server, client core, and apps are not yet started. Nothing here runs.

## How it works

- **Instances are islands.** Each one is a deployment: an Elixir server, a Postgres database, and a blob store. Instances never talk to each other. The client bridges them.
- **One identity, many accounts.** Your identity is a key hierarchy on your devices. Each instance you use holds an account bound to it. Joining a server on a new instance is a URL and a tap.
- **Two kinds of DM.** Server DMs belong to the server and follow its rules. Personal DMs belong to the people in them and are encrypted with MLS so no server, including ours, can read them.
- **Self-hosting is two containers.** The Lumia image and Postgres. Everything else is optional.
- **Open by design.** The server is AGPL-3.0 with no enterprise edition. The client core and protocol schemas are Apache-2.0. Bots, agents, and third-party clients use the same public API the official apps do, and every instance speaks MCP.

## Documents

| Document | What it covers |
|---|---|
| [docs/architecture.md](docs/architecture.md) | Every architectural decision, the alternatives rejected, and why. The hard rules in section 1 govern everything else. Start here. |
| [docs/protocol.md](docs/protocol.md) | The wire contract: encodings, the gateway, sequence numbers, idempotency, the HTTP mapping, errors, authentication, identity proof. |
| [docs/specs/](docs/specs/) | Implementation specs for identity, delivery, and authorization, each with its failure-injection tests. |
| [docs/plan.md](docs/plan.md) | The task checklist with status, and the decisions that changed along the way. |
| [proto/lumia/v1/](proto/lumia/v1/) | Protobuf schemas. The single source of truth for every shape on the wire; all client and server types are generated from them. |

## Repository layout

```
apps/instance   Elixir (Phoenix); what one deployment runs
apps/core       Rust client core: protocol, sync engine, local store, identity; lumia-cli
apps/ios        SwiftUI
apps/macos      SwiftUI
apps/android    Jetpack Compose
apps/desktop    Tauri shell for Windows and Linux
apps/web        Web UI (SolidJS), shared with apps/desktop
assets/         Logo, icons, fonts and design tokens shared by every shell
proto/          Protobuf schemas
docs/           Architecture, protocol contract, and specs
```

`apps/instance` and `apps/core` exist; the rest do not yet. Section 18 of the architecture doc gives the order they will be built in.

## Platforms

macOS, Windows, Linux, iOS, Android, and the browser. All clients share one Rust core; each platform gets a native shell.

## Contributing

Read [docs/architecture.md](docs/architecture.md) first. A change that conflicts with it needs to amend the document before it changes code. The protocol schemas only ever gain fields; see [docs/protocol.md](docs/protocol.md) section 3.

## License

Copyright (C) 2026 Ryan Winchester.

Server: AGPL-3.0. Client core and protocol schemas: Apache-2.0.

The root [LICENSE](LICENSE) is AGPL-3.0 and applies to everything not covered by a more specific license file. `apps/instance` carries its own copy of AGPL-3.0 because it ships as a container image. `proto/` and `apps/core` carry Apache-2.0. The Lumia name, logo and wordmark in `assets/brand` are not open source: they are the brand, all rights reserved, with the permitted uses in [assets/brand/LICENSE](assets/brand/LICENSE); a fork replaces them. The fonts in `assets/fonts` are SIL OFL.
