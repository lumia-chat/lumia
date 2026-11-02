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
    assert {:error, :unauthenticated} = Auth.refresh(tokens.refresh_token)
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
