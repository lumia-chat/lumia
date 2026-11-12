defmodule Lumia.AccountsTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Accounts.AccountEvent
  alias Lumia.Accounts.AccountEventPayload
  alias Lumia.Proto
  alias Lumia.V1

  defp profile_updated, do: {:profile_updated, %V1.ProfileUpdated{}}

  setup do
    %{account: account("ryan")}
  end

  describe "create_account/1" do
    test "handles are unique case-insensitively" do
      {:ok, _} = Accounts.create_account(%{handle: "Alice"})
      assert {:error, :already_exists} = Accounts.create_account(%{handle: "alice"})
    end

    test "a handle is 2 to 32 letters, digits, _, . and -, and does not begin with . or -" do
      for good <- ["ab", "Ryan", "ryan.w", "r_w-2", "_r", String.duplicate("a", 32)] do
        assert Accounts.valid_handle?(good), good
      end

      for bad <- ["a", "", " ab", "a b", ".ab", "-ab", "a@b", "ab\n", String.duplicate("a", 33)] do
        refute Accounts.valid_handle?(bad), inspect(bad)
      end

      assert {:error, changeset} = Accounts.create_account(%{handle: "no spaces"})
      assert %{handle: _} = errors_on(changeset)
    end
  end

  describe "update_account/2" do
    test "changes the display name, clears it, and says so on the account's log", %{
      account: account
    } do
      Phoenix.PubSub.subscribe(Lumia.PubSub, Accounts.topic(account.id))
      assert {:ok, named} = Accounts.update_account(account, %{display_name: "  Ryan W  "})
      assert named.display_name == "Ryan W" and named.handle == account.handle

      assert_receive {:account_event,
                      %V1.AccountEvent{
                        payload:
                          {:profile_updated,
                           %V1.ProfileUpdated{account: %V1.Account{display_name: "Ryan W"}}}
                      }, _bytes}

      assert {:ok, cleared} = Accounts.update_account(named, %{display_name: ""})
      assert cleared.display_name == nil
      # Nothing to change is nothing on the log.
      head = Accounts.head(account.id)
      assert {:ok, _} = Accounts.update_account(cleared, %{})
      assert {:ok, _} = Accounts.update_account(cleared, %{handle: cleared.handle})
      assert Accounts.head(account.id) == head

      long = String.duplicate("n", 65)
      assert {:error, :invalid_argument} = Accounts.update_account(cleared, %{display_name: long})
    end

    test "a new handle signs in, and one that is not a handle or is taken is refused" do
      {:ok, account} = Accounts.register(%{handle: "before", password: "correct horse battery"})
      other = account("other")

      assert {:error, :invalid_argument} = Accounts.update_account(account, %{handle: "no good"})
      assert {:error, :already_exists} = Accounts.update_account(account, %{handle: other.handle})

      assert {:error, :already_exists} =
               Accounts.update_account(account, %{handle: String.upcase(other.handle)})

      assert {:ok, renamed} = Accounts.update_account(account, %{handle: "After"})
      assert renamed.handle == "After"
      assert {:ok, %{id: id}} = Accounts.authenticate("after", "correct horse battery")
      assert id == account.id
      assert {:error, _} = Accounts.authenticate("before", "correct horse battery")
      # Only its case: the same handle to everyone else, so nothing is held.
      assert {:ok, recased} = Accounts.update_account(renamed, %{handle: "after"})
      assert recased.handle == "after"
      assert {:ok, _} = Accounts.create_account(%{handle: "unrelated"})
    end

    test "the handle left is held for the account that left it" do
      account = account("first")
      left = account.handle
      assert {:ok, renamed} = Accounts.update_account(account, %{handle: "second_#{left}"})

      # Nobody else can take it, by a rename or with a new account.
      other = account("other")
      assert {:error, :already_exists} = Accounts.update_account(other, %{handle: left})
      assert {:error, :already_exists} = Accounts.create_account(%{handle: String.upcase(left)})

      # A second rename holds the handle just left, and lets the first go.
      assert {:ok, again} = Accounts.update_account(renamed, %{handle: "third_#{left}"})
      assert {:error, :already_exists} = Accounts.update_account(other, %{handle: renamed.handle})
      assert {:ok, _} = Accounts.update_account(other, %{handle: left})

      # It can go back to the one it holds, and a hold runs out.
      assert {:ok, back} = Accounts.update_account(again, %{handle: renamed.handle})
      assert back.handle == renamed.handle

      Lumia.Repo.update_all(Lumia.Accounts.HandleHold,
        set: [until: DateTime.add(DateTime.utc_now(), -1, :second)]
      )

      assert {:ok, _} = Accounts.create_account(%{handle: again.handle})
    end

    test "rejects unknown kinds and instance roles" do
      assert {:error, changeset} = Accounts.create_account(%{handle: "xx", kind: "guest"})
      assert %{kind: _} = errors_on(changeset)

      assert {:error, changeset} = Accounts.create_account(%{handle: "yy", instance_role: "root"})
      assert %{instance_role: _} = errors_on(changeset)
    end
  end

  describe "append_event/3" do
    test "assigns dense, increasing seq per account", %{account: account} do
      seqs = for _ <- 1..3, do: elem(Accounts.append_event(account.id, profile_updated()), 1).seq
      assert seqs == [1, 2, 3]
    end

    test "writes event and serialized payload, advances the head, broadcasts", %{account: account} do
      Phoenix.PubSub.subscribe(Lumia.PubSub, Accounts.topic(account.id))

      payload =
        {:channel_removed, %V1.ChannelRemoved{channel_id: Proto.uuid(Ecto.UUID.generate())}}

      assert {:ok, %{seq: 1, event_id: event_id}} = Accounts.append_event(account.id, payload)

      assert %AccountEvent{id: ^event_id, kind: "channel_removed"} =
               Repo.get_by!(AccountEvent, account_id: account.id, seq: 1)

      %AccountEventPayload{payload: bytes} =
        Repo.get_by!(AccountEventPayload, account_id: account.id, seq: 1)

      assert %V1.AccountEvent{seq: 1, payload: ^payload} = V1.AccountEvent.decode(bytes)
      assert_receive {:account_event, %V1.AccountEvent{seq: 1}, ^bytes}

      account = Repo.reload!(account)
      assert account.last_seq == 1
      assert %DateTime{microsecond: {0, 0}} = account.last_seq_at
    end

    test "unknown account" do
      assert {:error, :account_not_found} =
               Accounts.append_event(Ecto.UUID.generate(), profile_updated())
    end
  end

  describe "events_after/3 and changed_since/1" do
    test "returns events strictly after the cursor, in order", %{account: account} do
      for _ <- 1..4, do: {:ok, _} = Accounts.append_event(account.id, profile_updated())
      assert Enum.map(Accounts.events_after(account.id, 2), & &1.seq) == [3, 4]
    end

    test "lists accounts whose head moved", %{account: account} do
      since = DateTime.add(DateTime.utc_now(), -1, :second)
      quiet = account("quiet")
      {:ok, _} = Accounts.append_event(account.id, profile_updated())

      changed = Accounts.changed_since(since)
      assert {account.id, 1} in changed
      refute Enum.any?(changed, fn {id, _} -> id == quiet.id end)
    end
  end

  test "account-log tables are hash-partitioned 32 ways" do
    for table <- ~w(account_events account_event_payloads) do
      %{rows: [[count]]} =
        Repo.query!("SELECT count(*) FROM pg_inherits WHERE inhparent = to_regclass($1::text)", [
          table
        ])

      assert count == 32, "#{table} has #{count} partitions"
    end
  end
end
