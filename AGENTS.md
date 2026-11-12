# Lumia

Chat app: self-hostable instances, each hosting many servers with channels and DMs. Distributed Elixir server, Postgres, Rust client core, native clients on macOS, Windows, Linux, iOS, Android, and web.

Read [docs/architecture.md](docs/architecture.md) before changing anything. It is the source of truth for architectural decisions, including the hard rules in section 1. If a change conflicts with it, amend the document first.

## Ownership

Claude owns this code and this repository, and keeps owning them from one session to the next (Ryan's decision, 2026-11-11). Ryan sets direction; the work is Claude's to carry. In practice:

- Decide and act where the architecture document and the plan already answer the question. Ask only for what is Ryan's to decide: a change of direction, anything that spends money or leaves this machine (a push, a release, an issue or pull request on someone else's repository), and the checks only a person can do.
- Keep [docs/plan.md](docs/plan.md) true as work lands: what is done, what is next, what was found. A future session starts from it, not from memory of this one.
- Leave every commit building, formatted, linted and tested for the parts it touches, and commit your own work on a branch as you go. Each app's `AGENTS.md` names its checks.
- Say plainly what was not verified. A screen nobody has typed into, a platform nothing has run on and a budget nobody has measured are open items in the plan, not done ones.
- Fix what you find. A bug met on the way is fixed with a test or written into the plan the same day; it is not left for whoever comes next, because that is you.
