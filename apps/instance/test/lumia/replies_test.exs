defmodule Lumia.RepliesTest do
  @moduledoc """
  Quote replies, reactions, mentions and read state (architecture 6.6).
  Not async: the push checks install a sender that reports here through
  the application's environment.
  """

  use Lumia.DataCase, async: false
  use Oban.Testing, repo: Lumia.Repo

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Authz.Access
  alias Lumia.Channels
  alias Lumia.Events
  alias Lumia.Events.EventPayload
  alias Lumia.Proto
  alias Lumia.Push
  alias Lumia.Reads
  alias Lumia.Servers
  alias Lumia.Snapshots
  alias Lumia.V1
  alias Lumia.Workers

  setup do
    Application.put_env(:lumia, :push_test_pid, self())
    on_exit(fn -> Application.delete_env(:lumia, :push_test_pid) end)
    owner = account("owner")
    grace = account("grace")
    ada = account("ada")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    {:ok, _} = Servers.add_member(server.id, grace.id, owner.id)
    {:ok, _} = Servers.add_member(server.id, ada.id, owner.id)
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

    command = %{command | op: {:send_message, op}}
    {:ok, ack} = Events.append(channel.id, actor(account), command)
    {Proto.from_uuid(command.command_id), ack}
  end

  defp mention(account),
    do: %V1.Mention{target: {:account_id, Proto.uuid(account.id)}, start: 0, end: 4}

  defp everyone, do: %V1.Mention{target: {:everyone, true}, start: 0, end: 9}

  defp quoting(message_id, text),
    do: %V1.QuoteReply{message_id: Proto.uuid(message_id), text: text}

  defp react(channel, account, message_id, emoji, op \\ :add_reaction) do
    command = %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel.id),
      op:
        case op do
          :add_reaction ->
            {:add_reaction, %V1.AddReaction{message_id: Proto.uuid(message_id), reaction: emoji}}

          :remove_reaction ->
            {:remove_reaction,
             %V1.RemoveReaction{message_id: Proto.uuid(message_id), reaction: emoji}}
        end
    }

    Events.append(channel.id, actor(account), command)
  end

  defp snapshot_message(channel, viewer, message_id) do
    snap = Snapshots.channel(%{channel: Repo.get!(Events.Channel, channel.id)}, viewer.id)
    Enum.find(snap.messages, &(Proto.from_uuid(&1.message_id) == message_id))
  end

  defp state(account, channel) do
    [state] = Reads.states(account.id, [channel.id])
    state
  end

  describe "quote replies" do
    test "a reply keeps the original's author and the text quoted, cut to its limit", ctx do
      {original, _} = say(ctx.general, ctx.grace, "Shipped on Friday, as planned.")
      long = String.duplicate("x", 2_500)
      {reply, _} = say(ctx.general, ctx.ada, "Nice.", reply_to: quoting(original, long))

      shown = snapshot_message(ctx.general, ctx.owner, reply)
      assert %V1.QuoteReply{text: text, original_deleted: false} = shown.reply_to
      assert Proto.from_uuid(shown.reply_to.author_account_id) == ctx.grace.id
      assert String.length(text) == 2_000

      # The live event carries it as the snapshot does.
      [event] = Events.events_after(ctx.general.id, ack_seq(ctx.general) - 1, 1)
      {:message_created, %V1.MessageCreated{message: live}} = event.payload
      assert Proto.from_uuid(live.reply_to.author_account_id) == ctx.grace.id
    end

    test "only a message of the same channel, there and not a notice, can be quoted", ctx do
      {:ok, design} = Servers.create_channel(ctx.server.id, ctx.owner.id, "design")
      {elsewhere, _} = say(design, ctx.owner, "Over here.")
      command = send_message(ctx.general.id, "Hm")
      {:send_message, op} = command.op

      command = %{
        command
        | op: {:send_message, %{op | reply_to: quoting(elsewhere, "Over here.")}}
      }

      assert {:error, :message_not_found} =
               Events.append(ctx.general.id, actor(ctx.owner), command)

      notice =
        Repo.one!(
          from m in Events.Message,
            where: m.channel_id == ^ctx.general.id and not is_nil(m.notice),
            limit: 1
        )

      command = %{
        command
        | command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
          op: {:send_message, %{op | reply_to: quoting(notice.id, "")}}
      }

      assert {:error, :message_not_found} =
               Events.append(ctx.general.id, actor(ctx.owner), command)
    end

    test "deleting the original drops the quoted text, in the row and in the stored event", ctx do
      {original, _} = say(ctx.general, ctx.grace, "A secret I regret.")

      {reply, %{seq: reply_seq}} =
        say(ctx.general, ctx.ada, "Ha!", reply_to: quoting(original, "A secret"))

      {:ok, _} =
        Events.append(ctx.general.id, actor(ctx.grace), delete_message(ctx.general.id, original))

      shown = snapshot_message(ctx.general, ctx.owner, reply)
      assert %V1.QuoteReply{text: "", original_deleted: true} = shown.reply_to

      bytes =
        Repo.one!(
          from p in EventPayload,
            where: p.channel_id == ^ctx.general.id and p.seq == ^reply_seq,
            select: p.payload
        )

      {:message_created, %V1.MessageCreated{message: stored}} = V1.Event.decode(bytes).payload
      assert %V1.QuoteReply{text: "", original_deleted: true} = stored.reply_to
      refute bytes =~ "A secret"
    end

    test "the person quoted is notified, unless it is the one replying", ctx do
      {original, _} = say(ctx.general, ctx.grace, "Ideas?")
      {_, %{seq: seq}} = say(ctx.general, ctx.ada, "One.", reply_to: quoting(original, "Ideas?"))
      [event] = Events.events_after(ctx.general.id, seq - 1, 1)
      assert Push.recipients(ctx.general.id, event) == [ctx.grace.id]

      {_, %{seq: seq}} =
        say(ctx.general, ctx.grace, "Me too.", reply_to: quoting(original, "Ideas?"))

      [event] = Events.events_after(ctx.general.id, seq - 1, 1)
      assert Push.recipients(ctx.general.id, event) == []
    end
  end

  describe "reactions" do
    test "any single emoji, and nothing else", _ctx do
      for emoji <- ["👍", "❤️", "👨‍👩‍👧", "🇨🇦", "1️⃣", "🏴‍☠️", "👍🏽", "✅"] do
        assert Lumia.Emoji.single?(emoji), emoji
      end

      for not_one <- ["", "ok", "a", "👍👍", "1", ":+1:", String.duplicate("👍", 40)] do
        refute Lumia.Emoji.single?(not_one), not_one
      end
    end

    test "reactions are kept and counted as each reader sees them", ctx do
      {message, _} = say(ctx.general, ctx.grace, "Lunch?")
      {:ok, _} = react(ctx.general, ctx.ada, message, "👍")
      {:ok, _} = react(ctx.general, ctx.owner, message, "👍")
      {:ok, _} = react(ctx.general, ctx.owner, message, "🍕")
      # Twice is once.
      {:ok, _} = react(ctx.general, ctx.owner, message, "🍕")

      assert {:error, :invalid_argument} = react(ctx.general, ctx.owner, message, "yes")

      assert {:error, :message_not_found} =
               react(ctx.general, ctx.owner, Ecto.UUID.generate(), "👍")

      [thumbs, pizza] = snapshot_message(ctx.general, ctx.ada, message).reactions
      assert %V1.ReactionCount{reaction: "👍", count: 2, me: true} = thumbs
      assert %V1.ReactionCount{reaction: "🍕", count: 1, me: false} = pizza
      assert Enum.map(thumbs.account_ids, &Proto.from_uuid/1) == [ctx.ada.id, ctx.owner.id]

      {:ok, _} = react(ctx.general, ctx.ada, message, "👍", :remove_reaction)
      [thumbs | _] = snapshot_message(ctx.general, ctx.ada, message).reactions
      assert %V1.ReactionCount{count: 1, me: false} = thumbs
    end
  end

  describe "mentions" do
    test "a mention's place in the text survives into snapshots", ctx do
      {message, _} = say(ctx.general, ctx.owner, "@ada look", mentions: [mention(ctx.ada)])
      shown = snapshot_message(ctx.general, ctx.grace, message)
      assert [%V1.Mention{target: {:account_id, id}, start: 0, end: 4}] = shown.mentions
      assert Proto.from_uuid(id) == ctx.ada.id
    end

    test "@everyone needs MENTION_EVERYONE, and notifies everyone in the channel", ctx do
      {:ok, access} = Access.require(ctx.grace.id, ctx.general.id, :send_messages)
      command = send_message(ctx.general.id, "@everyone hi")
      {:send_message, op} = command.op
      command = %{command | op: {:send_message, %{op | mentions: [everyone()]}}}
      assert {:error, :permission_denied} = Access.check_command(access, nil, command)

      {:ok, access} = Access.require(ctx.owner.id, ctx.general.id, :send_messages)
      assert :ok = Access.check_command(access, nil, command)
      {:ok, %{seq: seq}} = Events.append(ctx.general.id, actor(ctx.owner), command)
      [event] = Events.events_after(ctx.general.id, seq - 1, 1)

      assert Enum.sort(Push.recipients(ctx.general.id, event)) ==
               Enum.sort([ctx.grace.id, ctx.ada.id])
    end
  end

  describe "read state" do
    test "a newcomer finds nothing unread from before, and counts what comes after", ctx do
      {_, _} = say(ctx.general, ctx.owner, "Before you came.")
      bo = account("bo")
      {:ok, _} = Servers.add_member(ctx.server.id, bo.id, ctx.owner.id)
      assert %V1.ReadState{unread: 0, mentions: 0} = state(bo, ctx.general)

      {_, _} = say(ctx.general, ctx.owner, "After.")
      {_, _} = say(ctx.general, ctx.owner, "@bo you", mentions: [mention(bo)])
      {_, _} = say(ctx.general, bo, "My own messages are not unread.")
      assert %V1.ReadState{unread: 2, mentions: 1} = state(bo, ctx.general)

      # A quote of one's message notifies as a mention does.
      {mine, _} = say(ctx.general, bo, "Quote me.")
      {_, _} = say(ctx.general, ctx.owner, "Done.", reply_to: quoting(mine, "Quote me."))
      assert %V1.ReadState{unread: 3, mentions: 2} = state(bo, ctx.general)
    end

    test "reading moves forward only, tells the other devices, and wakes them", ctx do
      {_, %{seq: seq}} = say(ctx.general, ctx.owner, "One.")
      {_, %{seq: head}} = say(ctx.general, ctx.owner, "Two.")
      assert %V1.ReadState{unread: 2} = state(ctx.grace, ctx.general)

      assert {:ok, %V1.ReadState{read_seq: ^seq, unread: 1}} =
               Reads.mark(ctx.grace.id, ctx.general.id, seq)

      assert "read_position_updated" in Accounts.event_kinds(ctx.grace.id)
      assert_enqueued(worker: Workers.ReadPush, args: %{account_id: ctx.grace.id})

      # Past the head is the head; behind where it is changes nothing.
      assert {:ok, %V1.ReadState{read_seq: ^head, unread: 0}} =
               Reads.mark(ctx.grace.id, ctx.general.id, head + 50)

      assert {:ok, %V1.ReadState{read_seq: ^head}} = Reads.mark(ctx.grace.id, ctx.general.id, 1)

      outsider = account("outsider")
      assert {:error, :not_found} = Reads.mark(outsider.id, ctx.general.id, 1)
    end

    test "every message of a DM counts, server or personal", ctx do
      {:ok, dm} = Channels.open_dm(ctx.server.id, ctx.owner.id, ctx.grace.id)
      {_, _} = say(dm, ctx.owner, "Hi.")
      {_, _} = say(dm, ctx.owner, "Still there?")
      assert %V1.ReadState{unread: 2, mentions: 2} = state(ctx.grace, dm)

      %{channel: personal, a: a, b: b} = personal_dm()

      {:ok, _} =
        Events.append(personal.id, actor(a), send_encrypted(personal.id, :APPLICATION, 1))

      # A commit is not a message: only the two application events count.
      epoch = Repo.get!(Events.Channel, personal.id).mls_epoch
      control = send_encrypted(personal.id, :CONTROL, epoch + 1)
      {:ok, _} = Events.append(personal.id, actor(a), control)
      after_commit = send_encrypted(personal.id, :APPLICATION, epoch + 1)
      {:ok, _} = Events.append(personal.id, actor(a), after_commit)

      assert %V1.ReadState{unread: 2, mentions: 2} = state(b, personal)
      assert %V1.ReadState{unread: 0} = state(a, personal)
    end

    test "the snapshot carries the read states and the badge sums what notifies", ctx do
      {:ok, dm} = Channels.open_dm(ctx.server.id, ctx.owner.id, ctx.grace.id)
      {_, _} = say(dm, ctx.owner, "Hi.")
      {_, _} = say(ctx.general, ctx.owner, "@grace", mentions: [mention(ctx.grace)])
      {_, _} = say(ctx.general, ctx.owner, "Chatter.")

      states = Snapshots.account(ctx.grace.id).read_states

      assert Enum.any?(
               states,
               &(Proto.from_uuid(&1.channel_id) == ctx.general.id and &1.unread == 2)
             )

      assert Reads.badge(ctx.grace.id) == 2
    end

    test "Mentions lists what notifies the account, newest first, by server or all", ctx do
      {:ok, other} = Servers.create_server(ctx.owner.id, "Other")
      {:ok, lobby} = Servers.create_channel(other.id, ctx.owner.id, "lobby")
      {:ok, _} = Servers.add_member(other.id, ctx.grace.id, ctx.owner.id)
      {first, _} = say(ctx.general, ctx.owner, "@grace one", mentions: [mention(ctx.grace)])
      {_, _} = say(ctx.general, ctx.owner, "not for her")
      {second, _} = say(lobby, ctx.owner, "@everyone two", mentions: [everyone()])

      {all, false} = Reads.mentions(ctx.grace.id, nil)
      assert Enum.map(all, &Proto.from_uuid(&1.message_id)) == [second, first]

      {here, false} = Reads.mentions(ctx.grace.id, ctx.server.id)
      assert Enum.map(here, &Proto.from_uuid(&1.message_id)) == [first]

      {older, false} = Reads.mentions(ctx.grace.id, nil, second)
      assert Enum.map(older, &Proto.from_uuid(&1.message_id)) == [first]
    end
  end

  describe "pushes" do
    defp subscribe(account, device_id, kind) do
      :ok =
        Push.put(account.id, device_id, %V1.PushSubscription{
          kind: kind,
          endpoint: "https://push.example/#{device_id}",
          p256dh: Base.url_encode64(:crypto.strong_rand_bytes(65), padding: false),
          auth: Base.url_encode64(:crypto.strong_rand_bytes(16), padding: false)
        })
    end

    test "a notice carries the badge, and a read wakes only what can wake silently", ctx do
      phone = Ecto.UUID.generate()
      browser = Ecto.UUID.generate()
      subscribe(ctx.grace, phone, :RELAY_APNS)
      subscribe(ctx.grace, browser, :WEB_PUSH)

      {_, %{seq: seq}} = say(ctx.general, ctx.owner, "@grace", mentions: [mention(ctx.grace)])
      [event] = Events.events_after(ctx.general.id, seq - 1, 1)
      Push.deliver(ctx.general.id, event)
      assert_receive {:push, %{device_id: ^phone}, %{"body" => "New message", "badge" => 1}}
      assert_receive {:push, %{device_id: ^browser}, %{"badge" => 1}}

      {:ok, _} = Reads.mark(ctx.grace.id, ctx.general.id, seq)
      Push.deliver_read(ctx.grace.id)
      assert_receive {:push, %{device_id: ^phone}, %{"kind" => "read", "badge" => 0}}
      refute_receive {:push, %{device_id: ^browser}, %{"kind" => "read"}}
    end
  end

  defp ack_seq(channel), do: elem(Events.head(channel.id), 0)
end
