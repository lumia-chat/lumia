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
      assert {:error, changeset} = Accounts.create_account(%{handle: "alice"})
      assert %{handle: ["has already been taken"]} = errors_on(changeset)
    end

    test "rejects unknown kinds and instance roles" do
      assert {:error, changeset} = Accounts.create_account(%{handle: "x", kind: "guest"})
      assert %{kind: _} = errors_on(changeset)

      assert {:error, changeset} = Accounts.create_account(%{handle: "y", instance_role: "root"})
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
