This is the Lumia push relay (architecture 15.5): a Plug application that forwards content-free notices from instances to APNs and FCM with Lumia's credentials. Read `../../docs/architecture.md` before changing anything here.

## Rules

- The relay never sees message content and never stores anything: a token, an instance name and a time pass through and are logged at info level only as counts. Keep it that way.
- Providers implement `Lumia.Relay.Provider`; the fake one is what runs without credentials and in tests. A provider's request is built by a pure function (`request/…`) that the tests check; the send is the only I/O.
- Credentials come from the environment at boot (`config/runtime.exs`); none are in the repo.
- No `@spec`, `@type` or `@typedoc` (no Dialyzer). `mix precommit` must pass.
