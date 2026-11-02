defmodule Lumia.AccountsTest do
  use Lumia.DataCase, async: true

  alias Lumia.Accounts
  alias Lumia.Accounts.AccountEvent
  alias Lumia.Accounts.AccountEventPayload

  setup do
    {:ok, account} =
      Accounts.create_account(%{handle: "ryan#{System.unique_integer([:positive])}"})

    %{account: account}
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
      seqs =
        for _ <- 1..3, do: elem(Accounts.append_event(account.id, "profile_updated", %{}), 1).seq

      assert seqs == [1, 2, 3]
    end

    test "writes event and payload and advances the head", %{account: account} do
      assert {:ok, %{seq: 1, event_id: event_id}} =
               Accounts.append_event(account.id, "membership_added", %{"server_id" => "s"})

      assert %AccountEvent{id: ^event_id, kind: "membership_added"} =
               Repo.get_by!(AccountEvent, account_id: account.id, seq: 1)

      assert %AccountEventPayload{payload: %{"server_id" => "s"}} =
               Repo.get_by!(AccountEventPayload, account_id: account.id, seq: 1)

      account = Repo.reload!(account)
      assert account.last_seq == 1
      assert %DateTime{microsecond: {0, 0}} = account.last_seq_at
    end

    test "unknown account" do
      assert {:error, :account_not_found} = Accounts.append_event(Ecto.UUID.generate(), "x", %{})
    end
  end

  describe "events_after/3 and changed_since/1" do
    test "returns events strictly after the cursor, in order", %{account: account} do
      for _ <- 1..4, do: {:ok, _} = Accounts.append_event(account.id, "x", %{})
      assert Enum.map(Accounts.events_after(account.id, 2), & &1.seq) == [3, 4]
    end

    test "lists accounts whose head moved", %{account: account} do
      since = DateTime.add(DateTime.utc_now(), -1, :second)

      {:ok, quiet} =
        Accounts.create_account(%{handle: "quiet#{System.unique_integer([:positive])}"})

      {:ok, _} = Accounts.append_event(account.id, "x", %{})

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
