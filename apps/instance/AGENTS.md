This is the Lumia instance: the Elixir server that one deployment runs. It is a Phoenix application with no HTML surface; clients talk to it over the gateway and the JSON/protobuf API defined in `../../docs/protocol.md`. Read `../../docs/architecture.md` before changing anything here; its hard rules govern this code.

## Project guidelines

- Use `mix precommit` when you are done with all changes and fix any pending issues. It compiles with warnings as errors, checks formatting, runs Credo in strict mode, and runs the tests.
- Elixir 1.20 types are checked at compile time and type warnings are errors in CI. **No `@spec`, `@type`, `@typep`, `@opaque`, or `@typedoc` anywhere**, public functions included, and no Dialyzer: nothing verifies typespecs, so they could be wrong. Put contracts in `@doc` prose. CI greps for these attributes and fails.
- Every schema uses `use Lumia.Schema`: UUIDv7 primary keys minted app-side, UUID foreign keys, microsecond UTC timestamps. Never add a database default for an id column. Never derive a time from an id; `created_at` is always its own column.
- Migrations get UUID keys and microsecond timestamps from the Repo config. Partitioned tables (`events`, `event_payloads`, `commands`) are keyed on `(channel_id, ...)` and must stay that way so queries prune to one partition.
- The write path for channels is `Lumia.Events.append/2` and nothing else writes to the event-log tables. Broadcast happens after it returns, never inside it.
- Contexts open transactions with `Lumia.Tx.transact/1`, never `Repo.transact/1`, and broadcast through `Lumia.Tx.after_commit/1`. Ecto flattens nested transactions, so a broadcast made inside a nested one would fire before the outer commit; `Lumia.Tx` holds it until the outermost transaction commits and drops it on rollback. The test sandbox hides this bug, so a direct `Repo.transact` around an appending call will pass tests and still be wrong.
- `native/lumia_media` is the instance's one native library, a Rust crate Rustler builds when the instance compiles (`Lumia.Media`: image work on uploads, architecture 3 and 12), so a Rust toolchain is needed to build the instance; `native/lumia_media/rust-toolchain.toml` pins it. It forbids `unsafe`. Its rules are the architecture's: the format from the bytes, the bounds checked before decoding, the decode under an allocation limit, and nothing of an upload stored or served but what the instance encoded itself. Its own tests are `cargo test --release` there, over the fixtures in `test/support/fixtures/media`; add a format only if its decoder is Rust all the way down.
- Blobs go through `Lumia.Blobs` and nothing else writes to the blob store: it is what encrypts them under the master key and names them. A blob's address is a storage key and not an authorization, with one stated exception (avatars, architecture 12).
- Spike and benchmark mix tasks live in `bench/`, which compiles only in dev. Results go in `../../docs/spikes/`. Nothing in `bench/` ships.
- Use the already included `:req` (`Req`) library for HTTP requests; avoid `:httpoison`, `:tesla`, and `:httpc`.
- Never compare `DateTime` (or `Date`, `Time`) structs with `<`, `<=`, `>`, `>=`, including in guards: that is structural comparison and silently wrong. Use `DateTime.compare/2`, or push the comparison into the query.
- Phoenix.PubSub and Erlang distribution are the only coordination between nodes. No Redis, no broker; see architecture hard rule 2.
- We do not need new migrations to change a table. The app is still in development with no users. We can edit existing ones.

<!-- usage-rules-start -->

<!-- phoenix:elixir-start -->
## Elixir guidelines

- Elixir lists **do not support index based access via the access syntax**

  **Never do this (invalid)**:

      i = 0
      mylist = ["blue", "green"]
      mylist[i]

  Instead, **always** use `Enum.at`, pattern matching, or `List` for index based list access, ie:

      i = 0
      mylist = ["blue", "green"]
      Enum.at(mylist, i)

- Elixir variables are immutable, but can be rebound, so for block expressions like `if`, `case`, `cond`, etc
  you *must* bind the result of the expression to a variable if you want to use it and you CANNOT rebind the result inside the expression, ie:

      # INVALID: we are rebinding inside the `if` and the result never gets assigned
      if connected?(socket) do
        socket = assign(socket, :val, val)
      end

      # VALID: we rebind the result of the `if` to a new variable
      socket =
        if connected?(socket) do
          assign(socket, :val, val)
        end

- **Never** nest multiple modules in the same file as it can cause cyclic dependencies and compilation errors
- **Never** use map access syntax (`changeset[:field]`) on structs as they do not implement the Access behaviour by default. For regular structs, you **must** access the fields directly, such as `my_struct.field` or use higher level APIs that are available on the struct if they exist, `Ecto.Changeset.get_field/2` for changesets
- Elixir's standard library has everything necessary for date and time manipulation. Familiarize yourself with the common `Time`, `Date`, `DateTime`, and `Calendar` interfaces by accessing their documentation as necessary. **Never** install additional dependencies unless asked or for date/time parsing (which you can use the `date_time_parser` package)
- Don't use `String.to_atom/1` on user input (memory leak risk)
- Predicate function names should not start with `is_` and should end in a question mark. Names like `is_thing` should be reserved for guards
- Elixir's builtin OTP primitives like `DynamicSupervisor` and `Registry`, require names in the child spec, such as `{DynamicSupervisor, name: MyApp.MyDynamicSup}`, then you can use `DynamicSupervisor.start_child(MyApp.MyDynamicSup, child_spec)`
- Use `Task.async_stream(collection, callback, options)` for concurrent enumeration with back-pressure. The majority of times you will want to pass `timeout: :infinity` as option

## Mix guidelines

- Read the docs and options before using tasks (by using `mix help task_name`)
- To debug test failures, run tests in a specific file with `mix test test/my_test.exs` or run all previously failed tests with `mix test --failed`
- `mix deps.clean --all` is **almost never needed**. **Avoid** using it unless you have good reason

## Test guidelines

- **Always use `start_supervised!/1`** to start processes in tests as it guarantees cleanup between tests
- **Avoid** `Process.sleep/1` and `Process.alive?/1` in tests
  - Instead of sleeping to wait for a process to finish, **always** use `Process.monitor/1` and assert on the DOWN message:

      ref = Process.monitor(pid)
      assert_receive {:DOWN, ^ref, :process, ^pid, :normal}

   - Instead of sleeping to synchronize before the next call, **always** use `_ = :sys.get_state/1` to ensure the process has handled prior messages
<!-- phoenix:elixir-end -->

<!-- phoenix:phoenix-start -->
## Phoenix guidelines

- Remember Phoenix router `scope` blocks include an optional alias which is prefixed for all routes within the scope. **Always** be mindful of this when creating routes within a scope to avoid duplicate module prefixes.

- You **never** need to create your own `alias` for route definitions! The `scope` provides the alias, ie:

      scope "/admin", AppWeb.Admin do
        pipe_through :api

        get "/users", UserController, :index
      end

  the UserController route would point to the `AppWeb.Admin.UserController` module

- `Phoenix.View` no longer is needed or included with Phoenix, don't use it
<!-- phoenix:phoenix-end -->

<!-- phoenix:ecto-start -->
## Ecto Guidelines

- **Always** preload Ecto associations in queries when they'll be accessed by the caller
- Remember `import Ecto.Query` and other supporting modules when you write `seeds.exs`
- `Ecto.Schema` fields always use the `:string` type, even for `:text`, columns, ie: `field :name, :string`
- `Ecto.Changeset.validate_number/2` **DOES NOT SUPPORT the `:allow_nil` option**. By default, Ecto validations only run if a change for the given field exists and the change value is not nil, so such as option is never needed
- You **must** use `Ecto.Changeset.get_field(changeset, :field)` to access changeset fields
- Fields which are set programmatically, such as `user_id`, must not be listed in `cast` calls or similar for security purposes. Instead they must be explicitly set when creating the struct
- **Always** invoke `mix ecto.gen.migration migration_name_using_underscores` when generating migration files, so the correct timestamp and conventions are applied
<!-- phoenix:ecto-end -->

<!-- usage-rules-end -->
