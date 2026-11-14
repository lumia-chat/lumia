defmodule Lumia.RailsTest do
  @moduledoc "The rail as an account arranges it (architecture 6.9): kept tidied, told, refused when wrong."

  use LumiaWeb.ConnCase, async: true

  import Ecto.Query
  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.Proto
  alias Lumia.Rails
  alias Lumia.Servers
  alias Lumia.Snapshots
  alias Lumia.V1

  @proto "application/x-protobuf"

  setup do
    me = account("me")
    {:ok, a} = Servers.create_server(me.id, "A")
    {:ok, b} = Servers.create_server(me.id, "B")
    {:ok, c} = Servers.create_server(me.id, "C")
    %{me: me, a: a, b: b, c: c}
  end

  defp server(s), do: %V1.RailEntry{entry: {:server_id, Proto.uuid(s.id)}}

  defp folder(servers, opts \\ []) do
    %V1.RailEntry{
      entry:
        {:folder,
         %V1.RailFolder{
           folder_id: Keyword.get(opts, :id, Proto.uuid(Ecto.UUID.generate())),
           name: Keyword.get(opts, :name, ""),
           tone: Keyword.get(opts, :tone, "mint"),
           server_ids: Enum.map(servers, &Proto.uuid(&1.id))
         }}
    }
  end

  defp order(%V1.Rail{entries: entries}) do
    Enum.map(entries, fn
      %V1.RailEntry{entry: {:server_id, id}} -> Proto.from_uuid(id)
      %V1.RailEntry{entry: {:folder, f}} -> {:folder, Enum.map(f.server_ids, &Proto.from_uuid/1)}
    end)
  end

  test "with nothing arranged, the servers in the order they were joined", ctx do
    assert order(Rails.rail(ctx.me.id)) == [ctx.a.id, ctx.b.id, ctx.c.id]
  end

  test "an arrangement is kept, told to the account's devices, and in the snapshot", ctx do
    rail = %V1.Rail{entries: [server(ctx.c), folder([ctx.b, ctx.a], name: "  Work  ")]}
    assert {:ok, kept} = Rails.arrange(ctx.me.id, rail)
    assert order(kept) == [ctx.c.id, {:folder, [ctx.b.id, ctx.a.id]}]
    [%V1.RailEntry{entry: {:folder, folder}}] = tl(kept.entries)
    assert folder.name == "Work"

    assert [%V1.RailArranged{rail: ^kept}] =
             for(
               %V1.AccountEvent{payload: {:rail_arranged, arranged}} <-
                 Accounts.events_after(ctx.me.id, 0),
               do: arranged
             )

    assert order(Snapshots.account(ctx.me.id).rail) == order(kept)
  end

  test "it is kept tidied: strangers and repeats dropped, empty folders gone, the rest at the end",
       ctx do
    elsewhere = account("elsewhere")
    {:ok, theirs} = Servers.create_server(elsewhere.id, "Theirs")

    rail = %V1.Rail{
      entries: [server(theirs), server(ctx.b), folder([ctx.b, theirs]), folder([ctx.a, ctx.b])]
    }

    {:ok, kept} = Rails.arrange(ctx.me.id, rail)
    assert order(kept) == [ctx.b.id, {:folder, [ctx.a.id]}, ctx.c.id]

    # A server joined later lands at the end; one left goes, with no write.
    {:ok, d} = Servers.create_server(ctx.me.id, "D")
    :ok = leave(ctx.a, ctx.me)
    assert order(Rails.rail(ctx.me.id)) == [ctx.b.id, ctx.c.id, d.id]
  end

  test "a server left and joined again lands at the end, not back in its folder", ctx do
    friend = account("friend")
    {:ok, x} = Servers.create_server(friend.id, "X")
    {:ok, _} = Servers.add_member(x.id, ctx.me.id, friend.id)
    rail = %V1.Rail{entries: [folder([x, ctx.a], name: "Work"), server(ctx.b), server(ctx.c)]}
    assert {:ok, _} = Rails.arrange(ctx.me.id, rail)

    assert :ok = Servers.remove_member(x.id, ctx.me.id, ctx.me.id, "left")
    assert order(Rails.rail(ctx.me.id)) == [{:folder, [ctx.a.id]}, ctx.b.id, ctx.c.id]

    {:ok, _} = Servers.add_member(x.id, ctx.me.id, friend.id)
    assert order(Rails.rail(ctx.me.id)) == [{:folder, [ctx.a.id]}, ctx.b.id, ctx.c.id, x.id]
  end

  test "what tidying cannot make right is refused", ctx do
    long = String.duplicate("x", 51)
    same = Proto.uuid(Ecto.UUID.generate())

    assert {:error, {:invalid_argument, "name"}} =
             Rails.arrange(ctx.me.id, %V1.Rail{entries: [folder([ctx.a], name: long)]})

    assert {:error, {:invalid_argument, "tone"}} =
             Rails.arrange(ctx.me.id, %V1.Rail{entries: [folder([ctx.a], tone: "red")]})

    assert {:error, {:invalid_argument, "folder_id"}} =
             Rails.arrange(ctx.me.id, %V1.Rail{entries: [folder([ctx.a], id: "short")]})

    assert {:error, {:invalid_argument, "folder_id"}} =
             Rails.arrange(ctx.me.id, %V1.Rail{
               entries: [folder([ctx.a], id: same), folder([ctx.b], id: same)]
             })

    many = for _ <- 1..101, do: folder([ctx.a])

    assert {:error, {:invalid_argument, "folders"}} =
             Rails.arrange(ctx.me.id, %V1.Rail{entries: many})
  end

  test "PUT /account/rail answers what is kept, and a refusal names its field", ctx do
    {:ok, _session, tokens} = Auth.create_session(ctx.me.id, Ecto.UUID.generate())

    put = fn conn, rail ->
      conn
      |> put_req_header("authorization", "Bearer " <> tokens.access_token)
      |> put_req_header("accept", @proto)
      |> put_req_header("content-type", @proto)
      |> put("/api/v1/account/rail", V1.Rail.encode(rail))
    end

    ok = put.(ctx.conn, %V1.Rail{entries: [server(ctx.b)]})
    assert ok.status == 200
    assert order(V1.Rail.decode(ok.resp_body)) == [ctx.b.id, ctx.a.id, ctx.c.id]

    bad = put.(build_conn(), %V1.Rail{entries: [folder([ctx.a], tone: "red")]})
    assert bad.status == 400
    assert %V1.Error{details: %{"field" => "tone"}} = V1.Error.decode(bad.resp_body)
  end

  defp leave(server, account) do
    Lumia.Repo.delete_all(
      from m in Lumia.Servers.Member,
        where: m.server_id == ^server.id and m.account_id == ^account.id
    )

    :ok
  end
end
