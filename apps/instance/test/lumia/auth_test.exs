defmodule Lumia.AuthTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Auth.Session

  setup do
    account = account()
    device_id = Ecto.UUID.generate()
    {:ok, session, tokens} = Auth.create_session(account.id, device_id, client: "test/1")
    %{account: account, device_id: device_id, session: session, tokens: tokens}
  end

  test "stores only hashes", %{session: session, tokens: tokens} do
    refute session.access_hash == tokens.access_token
    assert byte_size(session.access_hash) == 32
    refute inspect(session) =~ "access_hash"
  end

  test "verifies a live access token", %{session: session, tokens: tokens} do
    assert {:ok, %Session{id: id}} = Auth.verify_access(tokens.access_token)
    assert id == session.id
  end

  test "rejects unknown, expired and revoked tokens", %{session: session, tokens: tokens} do
    assert {:error, :unauthenticated} = Auth.verify_access("nope")

    Repo.update!(
      Ecto.Changeset.change(session,
        access_expires_at: DateTime.add(DateTime.utc_now(), -1, :second)
      )
    )

    assert {:error, :token_expired} = Auth.verify_access(tokens.access_token)

    :ok = Auth.revoke(session.id)
    assert {:error, :session_revoked} = Auth.verify_access(tokens.access_token)
  end

  test "refresh rotates both tokens and invalidates the old refresh token", %{tokens: tokens} do
    assert {:ok, _session, new} = Auth.refresh(tokens.refresh_token)
    refute new.access_token == tokens.access_token
    assert {:ok, _} = Auth.verify_access(new.access_token)
    assert {:error, :unauthenticated} = Auth.verify_access(tokens.access_token)
    assert {:ok, _session, newer} = Auth.refresh(new.refresh_token)
    assert {:ok, _} = Auth.verify_access(newer.access_token)
  end

  # The grace is over for the session's retired token: its rotation is
  # moved back in time rather than waited for.
  defp past_grace(session) do
    long_ago = DateTime.add(DateTime.utc_now(), -(Auth.refresh_grace_seconds() + 1), :second)

    Repo.update!(
      Ecto.Changeset.change(Repo.reload!(session), previous_refresh_retired_at: long_ago)
    )
  end

  test "reusing a rotated refresh token after the grace revokes the whole session", ctx do
    assert {:ok, _session, new} = Auth.refresh(ctx.tokens.refresh_token)
    past_grace(ctx.session)

    # The retired token comes back: a leak. Everything the session issued dies.
    assert {:error, :session_revoked} = Auth.refresh(ctx.tokens.refresh_token)
    assert {:error, :session_revoked} = Auth.verify_access(new.access_token)
    assert {:error, :session_revoked} = Auth.refresh(new.refresh_token)
    assert Repo.reload!(ctx.session).revoked_at
    assert "session_revoked" in Accounts.event_kinds(ctx.account.id)

    # A token that was never issued is still just unknown.
    assert {:error, :unauthenticated} = Auth.refresh("never-issued")
  end

  test "a rotation whose answer was lost is given again within the grace", ctx do
    assert {:ok, _session, new} = Auth.refresh(ctx.tokens.refresh_token)
    rotated = Repo.reload!(ctx.session)

    # The client never saw `new` and asks again with what it holds: the
    # same pair, and nothing about the session has moved.
    assert {:ok, _session, again} = Auth.refresh(ctx.tokens.refresh_token)
    assert again == new
    assert Repo.reload!(ctx.session) == rotated
    refute "session_revoked" in Accounts.event_kinds(ctx.account.id)

    # And the pair works, once, like any other.
    assert {:ok, _} = Auth.verify_access(again.access_token)
    assert {:ok, _session, newer} = Auth.refresh(again.refresh_token)
    refute newer.refresh_token == again.refresh_token
  end

  test "two refreshes racing on one token are given the same pair", ctx do
    # Simulate the race: rotate once, then present the same token again
    # through the conditional update path, which finds the hash retired.
    assert {:ok, _, first} = Auth.refresh(ctx.tokens.refresh_token)
    assert {:ok, _, second} = Auth.refresh(ctx.tokens.refresh_token)
    assert first == second
  end

  test "the grace covers one rotation back and no further", ctx do
    assert {:ok, _, second} = Auth.refresh(ctx.tokens.refresh_token)
    assert {:ok, _, _third} = Auth.refresh(second.refresh_token)

    # The first token is two rotations back: no row remembers it, so it is
    # unknown, and the session is not revoked on its account.
    assert {:error, :unauthenticated} = Auth.refresh(ctx.tokens.refresh_token)
    refute Repo.reload!(ctx.session).revoked_at

    # The second is the retired one: inside the grace it gets the third again.
    assert {:ok, _, _} = Auth.refresh(second.refresh_token)
  end

  test "a retired token of a session rotated before the grace was kept is a reuse", ctx do
    assert {:ok, _, _} = Auth.refresh(ctx.tokens.refresh_token)

    Repo.update!(
      Ecto.Changeset.change(Repo.reload!(ctx.session), previous_refresh_retired_at: nil)
    )

    assert {:error, :session_revoked} = Auth.refresh(ctx.tokens.refresh_token)
  end

  test "revoke broadcasts to the session topic and appends to the account log", ctx do
    Phoenix.PubSub.subscribe(Lumia.PubSub, Auth.topic(ctx.session.id))
    assert :ok = Auth.revoke(ctx.session.id)
    assert_receive {:session_revoked, id}
    assert id == ctx.session.id
    assert "session_revoked" in Accounts.event_kinds(ctx.account.id)
    assert {:error, :not_found} = Auth.revoke(ctx.session.id)
  end

  test "revoke_device revokes every session of that device", ctx do
    {:ok, other, _} = Auth.create_session(ctx.account.id, ctx.device_id)
    {:ok, keep, keep_tokens} = Auth.create_session(ctx.account.id, Ecto.UUID.generate())

    Auth.revoke_device(ctx.account.id, ctx.device_id)

    assert Repo.reload!(ctx.session).revoked_at
    assert Repo.reload!(other).revoked_at
    assert {:ok, _} = Auth.verify_access(keep_tokens.access_token)
    assert [%Session{id: keep_id}] = Auth.list_sessions(ctx.account.id)
    assert keep_id == keep.id
  end
end
