defmodule LumiaWeb.CustomEmojiTest do
  @moduledoc """
  A server's custom emoji (specs/emoji.md): uploading, renaming and
  removing them under `MANAGE_EMOJI`, the limit the operator sets, their
  use in messages and as reactions, and who hears of a change.
  """

  use LumiaWeb.ConnCase, async: false
  use Oban.Testing, repo: Lumia.Repo

  import Ecto.Query
  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Auth
  alias Lumia.CustomEmoji
  alias Lumia.Events
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Snapshots
  alias Lumia.V1
  alias Lumia.Workers.EmojiUpdated
  alias Lumia.Workers.ServerUpdated

  @proto "application/x-protobuf"
  @media Path.expand("../../support/fixtures/media", __DIR__)
  @logo File.read!(Path.join(@media, "logo.png"))
  @gif File.read!(Path.join(@media, "animated.gif"))

  defp authed(conn, account) do
    {:ok, _session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())

    conn
    |> put_req_header("authorization", "Bearer " <> tokens.access_token)
    |> put_req_header("accept", @proto)
  end

  defp upload(conn, account, server, name, bytes \\ @logo) do
    conn
    |> authed(account)
    |> put_req_header("content-type", "image/png")
    |> post("/api/v1/servers/#{server.id}/emoji?name=#{URI.encode_www_form(name)}", bytes)
  end

  defp uploaded(conn, account, server, name, bytes \\ @logo) do
    res = upload(conn, account, server, name, bytes)
    assert res.status == 201, res.resp_body
    V1.CustomEmoji.decode(res.resp_body)
  end

  defp rename(conn, account, server, emoji, name) do
    conn
    |> authed(account)
    |> put_req_header("content-type", @proto)
    |> patch(
      "/api/v1/servers/#{server.id}/emoji/#{Proto.from_uuid(emoji.emoji_id)}",
      V1.UpdateEmojiRequest.encode(%V1.UpdateEmojiRequest{name: name})
    )
  end

  defp remove(conn, account, server, emoji) do
    conn
    |> authed(account)
    |> delete("/api/v1/servers/#{server.id}/emoji/#{Proto.from_uuid(emoji.emoji_id)}")
  end

  defp listed(conn, account, server) do
    res = conn |> authed(account) |> get("/api/v1/servers/#{server.id}/emoji")
    assert res.status == 200
    V1.ListEmojiResponse.decode(res.resp_body)
  end

  defp error(conn), do: V1.Error.decode(conn.resp_body)

  defp grant(ctx, account, permissions) do
    {:ok, role} =
      Servers.create_role(ctx.server.id, ctx.owner.id, %{
        name: "role #{System.unique_integer([:positive])}",
        permissions: permissions
      })

    :ok = Servers.assign_role(ctx.server.id, account.id, role.id, ctx.owner.id)
  end

  defp told_emoji(account) do
    for %V1.AccountEvent{payload: {:emoji_updated, %V1.EmojiUpdated{emoji: emoji}}} <-
          Accounts.events_after(account.id, 0),
        do: Enum.map(emoji, & &1.name)
  end

  defp run(worker) do
    for %Oban.Job{args: args} <- all_enqueued(worker: worker) do
      assert :ok = perform_job(worker, args)
    end

    Repo.delete_all(from j in Oban.Job, where: j.worker == ^inspect(worker))
  end

  defp say(channel, account, body, refs) do
    command = send_message(channel.id, body)
    {:send_message, op} = command.op
    command = %{command | op: {:send_message, %{op | emoji: refs}}}
    result = Events.append(channel.id, actor(account), command)
    {Proto.from_uuid(command.command_id), result}
  end

  defp ref(emoji, from, to), do: %V1.EmojiRef{emoji_id: emoji.emoji_id, start: from, end: to}

  defp react(channel, account, message_id, reaction, op \\ :add) do
    {reaction, emoji_id} =
      case reaction do
        %V1.CustomEmoji{emoji_id: id} -> {"", id}
        emoji -> {emoji, ""}
      end

    payload =
      case op do
        :add ->
          {:add_reaction,
           %V1.AddReaction{
             message_id: Proto.uuid(message_id),
             reaction: reaction,
             emoji_id: emoji_id
           }}

        :remove ->
          {:remove_reaction,
           %V1.RemoveReaction{
             message_id: Proto.uuid(message_id),
             reaction: reaction,
             emoji_id: emoji_id
           }}
      end

    command = %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel.id),
      op: payload
    }

    Events.append(channel.id, actor(account), command)
  end

  defp message(channel, viewer, message_id) do
    snap = Snapshots.channel(%{channel: Repo.get!(Events.Channel, channel.id)}, viewer.id)
    Enum.find(snap.messages, &(&1.message_id == Proto.uuid(message_id)))
  end

  setup do
    owner = account("owner")
    member = account("member")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    {:ok, _} = Servers.add_member(server.id, member.id, owner.id)
    %{owner: owner, member: member, server: server, general: general}
  end

  describe "managing them" do
    test "an upload is made by the instance, served to anyone, listed, renamed and removed",
         ctx do
      emoji = uploaded(ctx.conn, ctx.owner, ctx.server, "PartyParrot")

      assert %V1.CustomEmoji{name: "partyparrot", animated: false, width: 128, height: 85} =
               emoji

      assert "/api/v1/avatars/" <> _ = emoji.image_url
      served = get(build_conn(), emoji.image_url)
      assert served.status == 200
      assert <<0x89, "PNG", _::binary>> = served.resp_body

      assert %V1.ListEmojiResponse{emoji: [%{name: "partyparrot"}], limit: 50} =
               listed(ctx.conn, ctx.member, ctx.server)

      renamed = rename(ctx.conn, ctx.owner, ctx.server, emoji, "parrot")
      assert renamed.status == 200
      assert %V1.CustomEmoji{name: "parrot"} = V1.CustomEmoji.decode(renamed.resp_body)

      gone = remove(ctx.conn, ctx.owner, ctx.server, emoji)
      assert gone.status == 204
      assert get(build_conn(), emoji.image_url).status == 404
      assert %V1.ListEmojiResponse{emoji: []} = listed(ctx.conn, ctx.member, ctx.server)
      assert remove(ctx.conn, ctx.owner, ctx.server, emoji).status == 404

      actions =
        Repo.all(from a in Lumia.Audit.Entry, where: like(a.action, "emoji.%"), select: a.action)

      assert Enum.sort(actions) == ["emoji.create", "emoji.delete", "emoji.rename"]
    end

    test "an animated GIF stays animated, as a GIF", ctx do
      emoji = uploaded(ctx.conn, ctx.owner, ctx.server, "wave", @gif)
      assert %V1.CustomEmoji{animated: true, width: 128, height: 96} = emoji
      assert <<"GIF89a", _::binary>> = get(build_conn(), emoji.image_url).resp_body
    end

    test "need MANAGE_EMOJI, which a role gives and ADMINISTRATOR has", ctx do
      refused = upload(ctx.conn, ctx.member, ctx.server, "mine")
      assert refused.status == 403
      assert %V1.Error{code: :PERMISSION_DENIED} = error(refused)

      grant(ctx, ctx.member, [:manage_emoji])
      emoji = uploaded(ctx.conn, ctx.member, ctx.server, "mine")
      assert rename(ctx.conn, ctx.member, ctx.server, emoji, "ours").status == 200

      admin = account("admin")
      {:ok, _} = Servers.add_member(ctx.server.id, admin.id, ctx.owner.id)
      grant(ctx, admin, [:administrator])
      assert remove(ctx.conn, admin, ctx.server, emoji).status == 204

      stranger = account("stranger")
      assert upload(ctx.conn, stranger, ctx.server, "theirs").status == 404

      stranger_list =
        ctx.conn |> authed(stranger) |> get("/api/v1/servers/#{ctx.server.id}/emoji")

      assert stranger_list.status == 404
    end

    test "names are 2 to 32 of a-z, 0-9 and _, one live emoji's each", ctx do
      for name <- ["a", String.duplicate("a", 33), "has space", "dash-ed", "émoji", ""] do
        bad = upload(ctx.conn, ctx.owner, ctx.server, name)
        assert bad.status == 400, name
        assert %V1.Error{code: :INVALID_ARGUMENT, details: %{"field" => "name"}} = error(bad)
      end

      first = uploaded(ctx.conn, ctx.owner, ctx.server, "taken")
      again = upload(ctx.conn, ctx.owner, ctx.server, "TAKEN")
      assert again.status == 409
      first_id = Proto.from_uuid(first.emoji_id)

      assert %V1.Error{code: :ALREADY_EXISTS, details: %{"existing_id" => ^first_id}} =
               error(again)

      other = uploaded(ctx.conn, ctx.owner, ctx.server, "other")
      assert rename(ctx.conn, ctx.owner, ctx.server, other, "taken").status == 409

      # A removed emoji's name is free again at once.
      assert remove(ctx.conn, ctx.owner, ctx.server, first).status == 204
      assert %V1.CustomEmoji{name: "taken"} = uploaded(ctx.conn, ctx.owner, ctx.server, "taken")
    end

    test "what is not a picture is refused in words", ctx do
      bad = upload(ctx.conn, ctx.owner, ctx.server, "svg", "<svg onload=alert(1)/>")

      assert %V1.Error{details: %{"field" => "picture", "reason" => "unsupported"}} = error(bad)
      assert Repo.aggregate(CustomEmoji.Emoji, :count) == 0
    end
  end

  describe "the limit" do
    test "the default, then a server's own number from the operator; lowering deletes nothing",
         ctx do
      Application.put_env(:lumia, :emoji, server_max: 2)
      on_exit(fn -> Application.delete_env(:lumia, :emoji) end)

      a = uploaded(ctx.conn, ctx.owner, ctx.server, "one")
      _ = uploaded(ctx.conn, ctx.owner, ctx.server, "two")
      full = upload(ctx.conn, ctx.owner, ctx.server, "three")
      assert full.status == 507

      assert %V1.Error{code: :RESOURCE_EXHAUSTED, details: %{"state" => "emoji", "limit" => "2"}} =
               error(full)

      # A removed emoji counts for nothing.
      assert remove(ctx.conn, ctx.owner, ctx.server, a).status == 204
      _ = uploaded(ctx.conn, ctx.owner, ctx.server, "three")

      # The operator raises this server's limit; every member hears of it.
      run(ServerUpdated)

      assert {:ok, 3} =
               CustomEmoji.set_limit(ctx.server.id, 3)
               |> then(fn {:ok, s} -> {:ok, CustomEmoji.limit(s)} end)

      run(ServerUpdated)

      assert [%V1.Server{emoji_limit: 3}] =
               for(
                 %V1.AccountEvent{payload: {:server_updated, %V1.ServerUpdated{server: s}}} <-
                   Accounts.events_after(ctx.member.id, 0),
                 do: s
               )

      _ = uploaded(ctx.conn, ctx.owner, ctx.server, "four")
      assert listed(ctx.conn, ctx.owner, ctx.server).limit == 3

      # Lowered below what it has, it keeps them all and refuses new ones.
      {:ok, _} = CustomEmoji.set_limit(ctx.server.id, 1)
      assert length(listed(ctx.conn, ctx.owner, ctx.server).emoji) == 3
      assert upload(ctx.conn, ctx.owner, ctx.server, "five").status == 507

      {:ok, server} = CustomEmoji.set_limit(ctx.server.id, :default)
      assert server.emoji_limit == nil

      assert [%{actor_account_id: nil, override: true} | _] =
               Repo.all(from a in Lumia.Audit.Entry, where: a.action == "server.emoji_limit")
    end

    # Its success opens a connection of its own, outside the test's sandbox;
    # what it then does is `set_limit/2`, tested above.
    test "the operator's command refuses what is not a number or default", ctx do
      assert {:error, {:invalid_argument, "limit"}} =
               Lumia.Release.emoji_limit(ctx.server.id, "-1")

      assert {:error, {:invalid_argument, "limit"}} =
               Lumia.Release.emoji_limit(ctx.server.id, "x")

      assert {:error, :not_found} = Lumia.Release.emoji_limit("nope", "3")
    end
  end

  describe "telling the members" do
    test "EmojiUpdated reaches every member once for a burst, a guest too", ctx do
      guest = account("guest")
      {:ok, _} = Servers.add_member(ctx.server.id, guest.id, ctx.owner.id, kind: "guest")

      a = uploaded(ctx.conn, ctx.owner, ctx.server, "one")
      _ = uploaded(ctx.conn, ctx.owner, ctx.server, "two")
      assert rename(ctx.conn, ctx.owner, ctx.server, a, "uno").status == 200

      assert told_emoji(ctx.member) == []
      assert length(all_enqueued(worker: EmojiUpdated)) == 1
      run(EmojiUpdated)

      assert told_emoji(ctx.member) == [["uno", "two"]]
      assert told_emoji(guest) == [["uno", "two"]]
      assert told_emoji(ctx.owner) == [["uno", "two"]]
    end

    test "the account snapshot and MembershipAdded carry a server's emoji", ctx do
      _ = uploaded(ctx.conn, ctx.owner, ctx.server, "hello")
      assert [%V1.CustomEmoji{name: "hello"}] = Snapshots.account(ctx.member.id).emoji

      late = account("late")
      {:ok, _} = Servers.add_member(ctx.server.id, late.id, ctx.owner.id)

      assert [[%V1.CustomEmoji{name: "hello"}]] =
               for(
                 %V1.AccountEvent{payload: {:membership_added, %V1.MembershipAdded{emoji: e}}} <-
                   Accounts.events_after(late.id, 0),
                 do: e
               )
    end
  end

  describe "in messages" do
    test "refs are kept beside the text; a removed or another server's emoji is dropped", ctx do
      parrot = uploaded(ctx.conn, ctx.owner, ctx.server, "parrot")
      other_owner = account("other")
      {:ok, other} = Servers.create_server(other_owner.id, "Other")
      theirs = uploaded(ctx.conn, other_owner, other, "theirs")

      body = "hi :parrot: and :theirs:"

      {id, {:ok, _}} =
        say(ctx.general, ctx.member, body, [ref(parrot, 3, 11), ref(theirs, 16, 24)])

      assert %V1.Message{emoji: [%V1.EmojiRef{start: 3, end: 11} = kept]} =
               message(ctx.general, ctx.member, id)

      assert kept.emoji_id == parrot.emoji_id

      # Removed since: the message keeps its ref, and a new send drops it.
      assert remove(ctx.conn, ctx.owner, ctx.server, parrot).status == 204
      assert %V1.Message{emoji: [_]} = message(ctx.general, ctx.member, id)
      {later, {:ok, _}} = say(ctx.general, ctx.member, ":parrot:", [ref(parrot, 0, 8)])
      assert %V1.Message{emoji: []} = message(ctx.general, ctx.member, later)
    end

    test "a ref off a boundary, past the body, overlapping or not over :name: is refused",
         ctx do
      e = uploaded(ctx.conn, ctx.owner, ctx.server, "ok")

      for {body, refs} <- [
            {":ok:", [ref(e, 0, 5)]},
            {"é:ok:", [ref(e, 1, 6)]},
            {":ok::ok:", [ref(e, 0, 4), ref(e, 3, 8)]},
            {"hello", [ref(e, 0, 5)]},
            {":o k:", [ref(e, 0, 5)]}
          ] do
        assert {_, {:error, {:invalid_argument, "emoji"}}} =
                 say(ctx.general, ctx.member, body, refs),
               body
      end

      # A name other than the emoji's present one passes: a send made
      # just before a rename is not refused for it.
      assert {_, {:ok, _}} = say(ctx.general, ctx.member, ":old_name:", [ref(e, 0, 10)])
    end

    test "an edit carries its refs again", ctx do
      e = uploaded(ctx.conn, ctx.owner, ctx.server, "ok")
      {id, {:ok, _}} = say(ctx.general, ctx.member, "plain", [])

      edit = %V1.Command{
        command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
        channel_id: Proto.uuid(ctx.general.id),
        op:
          {:edit_message,
           %V1.EditMessage{message_id: Proto.uuid(id), body: "now :ok:", emoji: [ref(e, 4, 8)]}}
      }

      assert {:ok, _} = Events.append(ctx.general.id, actor(ctx.member), edit)

      assert %V1.Message{emoji: [%V1.EmojiRef{start: 4, end: 8}]} =
               message(ctx.general, ctx.member, id)
    end
  end

  describe "as reactions" do
    test "by id, named :name:, kept across a rename and after a removal", ctx do
      e = uploaded(ctx.conn, ctx.owner, ctx.server, "parrot")
      {id, {:ok, _}} = say(ctx.general, ctx.owner, "hello", [])

      assert {:ok, _} = react(ctx.general, ctx.member, id, e)
      assert {:ok, _} = react(ctx.general, ctx.owner, id, "👍")

      assert %V1.Message{reactions: [custom, thumbs]} = message(ctx.general, ctx.member, id)
      custom_of = fn message -> Enum.find(message.reactions, &(&1.emoji_id == e.emoji_id)) end
      assert %V1.ReactionCount{reaction: ":parrot:", count: 1, me: true} = custom
      assert custom.emoji_id == e.emoji_id
      assert %V1.ReactionCount{reaction: "👍", emoji_id: ""} = thumbs

      # Renamed: still one reaction, under the new name.
      assert rename(ctx.conn, ctx.owner, ctx.server, e, "bird").status == 200
      assert {:ok, _} = react(ctx.general, ctx.owner, id, e)

      assert %V1.ReactionCount{reaction: ":bird:", count: 2} =
               custom_of.(message(ctx.general, ctx.member, id))

      # Removed: counted as before by the name it had, none added, one's own taken away.
      assert remove(ctx.conn, ctx.owner, ctx.server, e).status == 204

      assert %V1.ReactionCount{reaction: ":bird:", count: 2} =
               custom_of.(message(ctx.general, ctx.member, id))

      newcomer = account("newcomer")
      {:ok, _} = Servers.add_member(ctx.server.id, newcomer.id, ctx.owner.id)
      assert {:error, {:invalid_argument, "emoji_id"}} = react(ctx.general, newcomer, id, e)
      assert {:ok, _} = react(ctx.general, ctx.member, id, e, :remove)

      assert %V1.ReactionCount{reaction: ":bird:", count: 1} =
               custom_of.(message(ctx.general, ctx.member, id))
    end

    test "the event names the emoji by id and :name:; another server's or both at once refused",
         ctx do
      e = uploaded(ctx.conn, ctx.owner, ctx.server, "parrot")
      {id, {:ok, _}} = say(ctx.general, ctx.owner, "hello", [])
      assert {:ok, _} = react(ctx.general, ctx.member, id, e)

      events = Events.events_after(ctx.general.id, 0, 50)

      assert [%V1.ReactionAdded{reaction: ":parrot:", emoji_id: emoji_id}] =
               for(%V1.Event{payload: {:reaction_added, r}} <- events, do: r)

      assert emoji_id == e.emoji_id

      other_owner = account("other")
      {:ok, other} = Servers.create_server(other_owner.id, "Other")
      theirs = uploaded(ctx.conn, other_owner, other, "theirs")

      assert {:error, {:invalid_argument, "emoji_id"}} =
               react(ctx.general, ctx.member, id, theirs)

      both = %V1.Command{
        command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
        channel_id: Proto.uuid(ctx.general.id),
        op:
          {:add_reaction,
           %V1.AddReaction{message_id: Proto.uuid(id), reaction: "👍", emoji_id: e.emoji_id}}
      }

      assert {:error, {:invalid_argument, "reaction"}} =
               Events.append(ctx.general.id, actor(ctx.member), both)
    end
  end
end
