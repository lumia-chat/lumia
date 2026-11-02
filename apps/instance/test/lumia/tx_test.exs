defmodule Lumia.TxTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Events
  alias Lumia.Tx
  alias Lumia.V1

  setup do
    account = account()
    Phoenix.PubSub.subscribe(Lumia.PubSub, Accounts.topic(account.id))
    %{account: account}
  end

  test "a top-level append broadcasts at once", %{account: account} do
    {:ok, _} = Accounts.append_event(account.id, {:profile_updated, %V1.ProfileUpdated{}})
    assert_receive {:account_event, %V1.AccountEvent{seq: 1}, _bytes}
  end

  test "an append inside an outer transaction broadcasts after the outer commit", ctx do
    {:ok, :done} =
      Tx.transact(fn ->
        {:ok, _} = Accounts.append_event(ctx.account.id, {:profile_updated, %V1.ProfileUpdated{}})
        # Still inside: nothing has reached the topic yet.
        refute_received {:account_event, _, _}
        assert Tx.open?()
        {:ok, :done}
      end)

    refute Tx.open?()
    assert_receive {:account_event, %V1.AccountEvent{seq: 1}, _bytes}
  end

  test "an outer rollback drops the broadcast and the seq is reused", ctx do
    {:error, :nope} =
      Tx.transact(fn ->
        {:ok, %{seq: 1}} =
          Accounts.append_event(ctx.account.id, {:profile_updated, %V1.ProfileUpdated{}})

        {:error, :nope}
      end)

    refute_receive {:account_event, _, _}, 50
    assert Accounts.head(ctx.account.id) == 0

    # The next real event takes seq 1; no client ever saw a phantom seq 1.
    {:ok, %{seq: 1}} =
      Accounts.append_event(ctx.account.id, {:profile_updated, %V1.ProfileUpdated{}})

    assert_receive {:account_event, %V1.AccountEvent{seq: 1}, _bytes}
  end

  test "a raise inside the transaction drops the queue and leaves no depth behind", ctx do
    assert_raise RuntimeError, fn ->
      Tx.transact(fn ->
        {:ok, _} = Accounts.append_event(ctx.account.id, {:profile_updated, %V1.ProfileUpdated{}})
        raise "boom"
      end)
    end

    refute Tx.open?()
    refute_receive {:account_event, _, _}, 50
  end

  test "channel events and session revocations defer the same way", ctx do
    channel = channel()
    Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(channel.id))
    {:ok, session, tokens} = Lumia.Auth.create_session(ctx.account.id, Ecto.UUID.generate())
    Phoenix.PubSub.subscribe(Lumia.PubSub, Lumia.Auth.topic(session.id))

    {:error, :nope} =
      Tx.transact(fn ->
        {:ok, _} =
          Events.append_system(channel.id, {:acl_changed, %V1.AclChanged{acl_version: 1}})

        :ok = Lumia.Auth.revoke(session.id)
        {:error, :nope}
      end)

    refute_receive {:event, _, _}, 50
    refute_receive {:session_revoked, _}, 50
    assert Events.head(channel.id) == {0, 1}
    # The revocation rolled back with everything else.
    assert {:ok, _} = Lumia.Auth.verify_access(tokens.access_token)
  end
end
