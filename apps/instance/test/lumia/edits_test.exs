defmodule Lumia.EditsTest do
  @moduledoc """
  Editing and deleting messages, and replies that notify nobody
  (architecture 6.6): an edit is its author's alone, a delete its author's
  or a `MANAGE_MESSAGES` holder's, a notice neither; an edit carries the
  message's mentions again and keeps the quoted author notified as it was;
  a silent reply notifies the author it quotes of nothing.
  """

  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Authz.Access
  alias Lumia.Events
  alias Lumia.Events.Message
  alias Lumia.Proto
  alias Lumia.Reads
  alias Lumia.Servers
  alias Lumia.Snapshots
  alias Lumia.V1

  setup do
    owner = account("owner")
    grace = account("grace")
    ada = account("ada")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    {:ok, _} = Servers.add_member(server.id, grace.id, owner.id)
    {:ok, _} = Servers.add_member(server.id, ada.id, owner.id)
    for a <- [grace, ada], do: {:ok, _} = Lumia.Channels.join(general.id, a.id)
    %{owner: owner, grace: grace, ada: ada, server: server, general: general}
  end

  defp say(channel, account, body, opts \\ []) do
    command = send_message(channel.id, body)
    {:send_message, op} = command.op

    op = %{
      op
      | mentions: Keyword.get(opts, :mentions, []),
        reply_to: Keyword.get(opts, :reply_to)
    }

    {:ok, _} = Events.append(channel.id, actor(account), %{command | op: {:send_message, op}})
    Proto.from_uuid(command.command_id)
  end

  defp command(channel, op) do
    %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel.id),
      op: op
    }
  end

  defp edit(channel, message_id, body, mentions \\ []) do
    command(
      channel,
      {:edit_message,
       %V1.EditMessage{message_id: Proto.uuid(message_id), body: body, mentions: mentions}}
    )
  end

  defp delete(channel, message_id),
    do: command(channel, {:delete_message, %V1.DeleteMessage{message_id: Proto.uuid(message_id)}})

  # What the gateway and the HTTP command path ask before appending.
  defp allowed(account, channel, %V1.Command{op: {op, _}} = command) do
    needed = if op == :delete_message, do: :view_channel, else: :send_messages

    with {:ok, access} <- Access.require(account.id, channel.id, needed) do
      Access.check_command(access, account.id, command)
    end
  end

  defp mention(account),
    do: %V1.Mention{target: {:account_id, Proto.uuid(account.id)}, start: 0, end: 4}

  defp quoting(message_id, text, silent \\ false),
    do: %V1.QuoteReply{message_id: Proto.uuid(message_id), text: text, silent: silent}

  test "an author edits their own message and nobody else may", ctx do
    message = say(ctx.general, ctx.grace, "Shipped on Thursday.")
    by_author = edit(ctx.general, message, "Shipped on Friday.")
    assert :ok = allowed(ctx.grace, ctx.general, by_author)
    # Not even the owner, who holds every permission.
    assert {:error, :permission_denied} =
             allowed(ctx.owner, ctx.general, edit(ctx.general, message, "No."))

    assert {:error, :permission_denied} =
             allowed(ctx.ada, ctx.general, edit(ctx.general, message, "No."))

    {:ok, _} = Events.append(ctx.general.id, actor(ctx.grace), by_author)
    row = Repo.get!(Message, message)
    assert row.body == "Shipped on Friday."
    assert row.edited_at
  end

  test "an author or a moderator deletes a message, and nobody else", ctx do
    message = say(ctx.general, ctx.grace, "Oops.")

    assert {:error, :permission_denied} =
             allowed(ctx.ada, ctx.general, delete(ctx.general, message))

    assert :ok = allowed(ctx.grace, ctx.general, delete(ctx.general, message))
    # The owner holds MANAGE_MESSAGES.
    assert :ok = allowed(ctx.owner, ctx.general, delete(ctx.general, message))

    {:ok, _} = Events.append(ctx.general.id, actor(ctx.owner), delete(ctx.general, message))
    assert Repo.get!(Message, message).deleted_at
    # The row keeps the words for moderation; history gives a reader the tombstone.
    %V1.ListMessagesResponse{messages: [shown]} =
      Snapshots.messages(ctx.general.id, 0, 50, nil, ctx.ada.id)
      |> Map.update!(:messages, fn ms -> Enum.filter(ms, &(&1.notice == nil)) end)

    assert shown.deleted_at
    assert shown.body == ""
    assert shown.mentions == [] and shown.reply_to == nil and shown.reactions == []
    # Gone: nothing more to edit or delete.
    assert {:error, :message_not_found} =
             allowed(ctx.grace, ctx.general, edit(ctx.general, message, "Back?"))
  end

  test "a notice is neither edited nor deleted", ctx do
    notice =
      Repo.one!(
        from m in Message,
          where: m.channel_id == ^ctx.general.id and not is_nil(m.notice),
          order_by: [desc: m.seq],
          limit: 1,
          select: m.id
      )

    assert {:error, :message_not_found} =
             allowed(ctx.ada, ctx.general, edit(ctx.general, notice, "Edited"))

    assert {:error, :message_not_found} =
             allowed(ctx.owner, ctx.general, delete(ctx.general, notice))
  end

  test "an edit mentioning everyone needs MENTION_EVERYONE", ctx do
    message = say(ctx.general, ctx.grace, "Hello")
    everyone = %V1.Mention{target: {:everyone, true}, start: 0, end: 9}

    assert {:error, :permission_denied} =
             allowed(ctx.grace, ctx.general, edit(ctx.general, message, "@everyone", [everyone]))
  end

  test "an edit carries the mentions again and keeps the quoted author notified", ctx do
    original = say(ctx.general, ctx.grace, "Lunch?")

    reply =
      say(ctx.general, ctx.ada, "@own yes",
        mentions: [mention(ctx.owner)],
        reply_to: quoting(original, "Lunch?")
      )

    assert Enum.sort(Repo.get!(Message, reply).mentioned_account_ids) ==
             Enum.sort([ctx.owner.id, ctx.grace.id])

    {:ok, _} = Events.append(ctx.general.id, actor(ctx.ada), edit(ctx.general, reply, "yes"))
    row = Repo.get!(Message, reply)
    assert row.mentioned_account_ids == [ctx.grace.id]
    assert row.mention_spans == []
  end

  test "a silent reply notifies the author it quotes of nothing", ctx do
    original = say(ctx.general, ctx.grace, "Lunch?")
    before = Reads.states(ctx.grace.id, [ctx.general.id]) |> hd()
    reply = say(ctx.general, ctx.ada, "Yes", reply_to: quoting(original, "Lunch?", true))

    assert Repo.get!(Message, reply).mentioned_account_ids == []
    after_reply = Reads.states(ctx.grace.id, [ctx.general.id]) |> hd()
    assert after_reply.mentions == before.mentions
    assert after_reply.unread == before.unread + 1

    snap = Snapshots.channel(%{channel: Repo.get!(Events.Channel, ctx.general.id)}, ctx.grace.id)
    shown = Enum.find(snap.messages, &(Proto.from_uuid(&1.message_id) == reply))
    assert %V1.QuoteReply{silent: true} = shown.reply_to

    loud = say(ctx.general, ctx.ada, "Yes!", reply_to: quoting(original, "Lunch?"))
    snap = Snapshots.channel(%{channel: Repo.get!(Events.Channel, ctx.general.id)}, ctx.grace.id)
    shown = Enum.find(snap.messages, &(Proto.from_uuid(&1.message_id) == loud))
    assert %V1.QuoteReply{silent: false} = shown.reply_to
  end
end
