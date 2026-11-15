defmodule LumiaWeb.LinkSessionsApiTest do
  @moduledoc "Linking by prompt (specs/identity.md 4.2.1): the relay, step by step."

  use LumiaWeb.ConnCase, async: true

  import Lumia.IdentityFixtures

  alias Lumia.Accounts
  alias Lumia.Identity
  alias Lumia.Proto
  alias Lumia.V1

  @proto "application/x-protobuf"

  defp post_proto(conn, path, struct) do
    conn
    |> put_req_header("content-type", @proto)
    |> put_req_header("accept", @proto)
    |> post(path, struct.__struct__.encode(struct))
  end

  defp authed(conn, account, device_id) do
    {:ok, _, tokens} = Lumia.Auth.create_session(account.id, device_id)
    put_req_header(conn, "authorization", "Bearer " <> tokens.access_token)
  end

  defp session_of(resp), do: V1.LinkSession.decode(resp.resp_body)

  defp commit(reveal), do: Lumia.Crypto.blake3("lumia/link-commit/v1" <> reveal)

  defp request do
    V1.LinkRequest.encode(%V1.LinkRequest{
      device_id: :crypto.strong_rand_bytes(16),
      ephemeral_public_key: :crypto.strong_rand_bytes(32),
      kind: :NATIVE,
      label: "Pixel 9"
    })
  end

  # An account whose identity is held here, a device of it signed in (E), and
  # a new device signed in by credentials with no identity (N).
  defp setting(conn) do
    %{identity: identity, device: phone, roster: v1} = bootstrap()

    {:ok, account} =
      Accounts.register(%{
        handle: "ls#{System.unique_integer([:positive])}",
        password: "correct horse battery",
        identity_id: identity.identity_id
      })

    {:ok, _} = Identity.accept_roster(v1, :publish)

    %{
      account: account,
      e: authed(conn, account, Proto.from_uuid(phone.id)),
      n: authed(conn, account, Ecto.UUID.generate())
    }
  end

  defp open(n, reveal) do
    post_proto(n, "/api/v1/identity/link-sessions", %V1.OpenLinkSessionRequest{
      request: request(),
      commitment: commit(reveal)
    })
  end

  test "open, answer, reveal and code, each once, in order, by the right session", %{conn: conn} do
    s = setting(conn)
    reveal = :crypto.strong_rand_bytes(32)
    opened = open(s.n, reveal)
    assert opened.status == 201
    %V1.LinkSession{session_id: id} = session_of(opened)
    path = "/api/v1/identity/link-sessions/#{Proto.from_uuid(id)}"
    assert "link_session_updated" in Accounts.event_kinds(s.account.id)

    listed = s.e |> put_req_header("accept", @proto) |> get("/api/v1/identity/link-sessions")

    assert [%V1.LinkSession{session_id: ^id}] =
             V1.ListLinkSessionsResponse.decode(listed.resp_body).sessions

    # Revealing before an answer is out of order.
    assert post_proto(s.n, path <> "/reveal", %V1.RevealLinkSessionRequest{reveal: reveal}).status ==
             400

    answer = :crypto.strong_rand_bytes(32)
    answered = post_proto(s.e, path <> "/accept", %V1.AnswerLinkSessionRequest{answer: answer})
    assert %V1.LinkSession{answer: ^answer} = session_of(answered)
    # One answer.
    again = post_proto(s.e, path <> "/accept", %V1.AnswerLinkSessionRequest{answer: answer})

    assert %V1.Error{code: :FAILED_PRECONDITION, details: %{"state" => "answered"}} =
             V1.Error.decode(again.resp_body)

    # Only the session that opened it reveals, and only what it committed to.
    assert post_proto(s.e, path <> "/reveal", %V1.RevealLinkSessionRequest{reveal: reveal}).status ==
             403

    wrong = :crypto.strong_rand_bytes(32)

    assert post_proto(s.n, path <> "/reveal", %V1.RevealLinkSessionRequest{reveal: wrong}).status ==
             400

    revealed = post_proto(s.n, path <> "/reveal", %V1.RevealLinkSessionRequest{reveal: reveal})
    assert %V1.LinkSession{reveal: ^reveal} = session_of(revealed)

    # Only the answering session gives the code.
    assert post_proto(s.n, path <> "/code", %V1.LinkSessionCodeRequest{code: "K7QM2XRA"}).status ==
             403

    coded = post_proto(s.e, path <> "/code", %V1.LinkSessionCodeRequest{code: "K7QM2XRA"})
    assert %V1.LinkSession{code: "K7QM2XRA"} = session_of(coded)

    # Done: no longer offered as a prompt, still read by its id.
    listed = s.e |> put_req_header("accept", @proto) |> get("/api/v1/identity/link-sessions")
    assert V1.ListLinkSessionsResponse.decode(listed.resp_body).sessions == []
    fetched = s.n |> put_req_header("accept", @proto) |> get(path)
    assert %V1.LinkSession{code: "K7QM2XRA"} = session_of(fetched)
  end

  test "only a device of the identity answers, a declined session takes nothing, and another account's is not found",
       %{conn: conn} do
    s = setting(conn)
    %V1.LinkSession{session_id: id} = session_of(open(s.n, :crypto.strong_rand_bytes(32)))
    path = "/api/v1/identity/link-sessions/#{Proto.from_uuid(id)}"
    answer = %V1.AnswerLinkSessionRequest{answer: :crypto.strong_rand_bytes(32)}

    # The new device, not in the roster, cannot answer its own request.
    assert post_proto(s.n, path <> "/accept", answer).status == 403

    declined = post_proto(s.e, path <> "/decline", %V1.AnswerLinkSessionRequest{})
    assert %V1.LinkSession{declined: true} = session_of(declined)
    assert post_proto(s.e, path <> "/accept", answer).status == 400

    other = setting(conn)
    assert other.e |> put_req_header("accept", @proto) |> get(path) |> Map.get(:status) == 404
  end

  test "a request that is not one is refused, and an account has five open at most", %{conn: conn} do
    s = setting(conn)

    bad =
      post_proto(s.n, "/api/v1/identity/link-sessions", %V1.OpenLinkSessionRequest{
        request: "not a request",
        commitment: commit("x")
      })

    assert bad.status == 400

    for _ <- 1..5, do: assert(open(s.n, :crypto.strong_rand_bytes(32)).status == 201)
    assert open(s.n, :crypto.strong_rand_bytes(32)).status == 429
  end
end
