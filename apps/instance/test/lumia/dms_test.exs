defmodule Lumia.DmsTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Authz.Access
  alias Lumia.Dms
  alias Lumia.Snapshots
  alias Lumia.V1

  describe "requests" do
    test "request then accept opens the channel, with the acceptance on both logs first" do
      a = account("a")
      b = account("b")

      assert {:ok, request} = Dms.send_request(a.id, b.id)
      assert request.state == "pending"
      assert "dm_request_received" in Accounts.event_kinds(b.id)
      # Sending again, in either direction, returns the pending one.
      assert {:ok, %{id: same}} = Dms.send_request(a.id, b.id)
      assert {:ok, %{id: ^same}} = Dms.send_request(b.id, a.id)
      assert same == request.id

      # Only the recipient answers.
      assert {:error, :not_found} = Dms.respond(request.id, a.id, :accepted)
      assert {:error, :not_found} = Dms.respond(Ecto.UUID.generate(), b.id, :accepted)

      assert {:ok, %{state: "accepted"}, channel} = Dms.respond(request.id, b.id, :accepted)
      assert channel.kind == "personal_dm"
      assert Dms.participants(channel.id) == Enum.sort([a.id, b.id])
      assert Dms.accepted_request?(b.id, a.id)

      for id <- [a.id, b.id] do
        kinds = Accounts.event_kinds(id)
        assert "dm_request_updated" in kinds and "channel_added" in kinds

        assert Enum.find_index(kinds, &(&1 == "dm_request_updated")) <
                 Enum.find_index(kinds, &(&1 == "channel_added"))
      end

      # Answered once; a shared channel makes a new request pointless.
      assert {:error, :failed_precondition} = Dms.respond(request.id, b.id, :declined)
      assert {:error, :already_exists} = Dms.send_request(a.id, b.id)
    end

    test "decline creates nothing; block hides the recipient from the sender" do
      a = account("a")
      b = account("b")
      c = account("c")
      assert {:error, :invalid_argument} = Dms.send_request(a.id, a.id)
      assert {:error, :account_not_found} = Dms.send_request(a.id, Ecto.UUID.generate())

      {:ok, declined} = Dms.send_request(a.id, b.id)
      assert {:ok, %{state: "declined"}, nil} = Dms.respond(declined.id, b.id, :declined)
      refute "channel_added" in Accounts.event_kinds(a.id)
      refute Dms.accepted_request?(a.id, b.id)

      {:ok, blocked} = Dms.send_request(a.id, c.id)
      assert {:ok, %{state: "blocked"}, nil} = Dms.respond(blocked.id, c.id, :blocked)
      assert {:error, :not_found} = Dms.send_request(a.id, c.id)
      assert Dms.pending_requests_to(c.id) == []
    end

    test "the snapshot lists requests waiting for the account" do
      a = account("a")
      b = account("b")
      {:ok, request} = Dms.send_request(a.id, b.id)

      assert %V1.AccountSnapshot{pending_requests: [%V1.DmRequest{state: :PENDING} = r]} =
               Snapshots.account(b.id)

      assert Lumia.Proto.from_uuid(r.request_id) == request.id
      assert %V1.AccountSnapshot{pending_requests: []} = Snapshots.account(a.id)
    end
  end

  describe "open_personal/2" do
    test "needs an accepted request, then creates once and returns the existing channel" do
      a = account("a")
      b = account("b")

      assert {:error, {:failed_precondition, "request"}} = Dms.open_personal(a.id, [b.id])

      {:ok, request} = Dms.send_request(b.id, a.id)
      {:ok, _, channel} = Dms.respond(request.id, a.id, :accepted)
      assert channel.kind == "personal_dm"
      assert is_nil(channel.server_id)

      assert {:ok, %{id: same}, true} = Dms.open_personal(b.id, [a.id])
      assert same == channel.id
      # Including yourself in the list is fine.
      assert {:ok, %{id: ^same}, true} = Dms.open_personal(a.id, [a.id, b.id])
      assert {:error, :invalid_argument} = Dms.open_personal(a.id, [])
    end

    test "a group DM needs the gate with every participant and is never deduplicated" do
      [a, b, c, stranger] = for n <- ~w(a b c s), do: account(n)
      %{} = personal_dm(a, b)
      %{} = personal_dm(a, c)

      assert {:error, {:failed_precondition, "request"}} =
               Dms.open_personal(a.id, [b.id, stranger.id])

      assert {:ok, group, false} = Dms.open_personal(a.id, [b.id, c.id])
      assert group.kind == "personal_group_dm"
      assert is_nil(group.dm_key)
      assert Dms.participants(group.id) == Enum.sort([a.id, b.id, c.id])

      for id <- [a.id, b.id, c.id] do
        assert Enum.count(Accounts.event_kinds(id), &(&1 == "channel_added")) >= 2
      end

      assert {:ok, %{id: other}, false} = Dms.open_personal(a.id, [b.id, c.id])
      refute other == group.id
      assert {:ok, %{}} = Access.channel(c.id, group.id)
      assert {:error, :not_found} = Access.channel(stranger.id, group.id)
    end
  end

  describe "access" do
    test "participants hold everything; anyone else sees not_found" do
      %{channel: channel, a: a, b: b} = personal_dm()
      stranger = account("stranger")

      assert {:ok, %{permissions: bits}} = Access.require(a.id, channel.id, :send_messages)
      assert bits == Lumia.Authz.Permission.all()
      assert {:ok, _} = Access.viewable_channel(b.id, channel.id)
      assert {:error, :not_found} = Access.channel(stranger.id, channel.id)

      assert Dms.shares_personal_channel?(a.id, b.id)
      assert Dms.shares_personal_channel?(b.id, a.id)
      refute Dms.shares_personal_channel?(a.id, stranger.id)
      refute Dms.participant?(channel.id, stranger.id)
    end

    test "the snapshot carries encrypted events, not messages" do
      %{channel: channel, a: a} = personal_dm()
      actor = actor(a)

      {:ok, _} = Lumia.Events.append(channel.id, actor, send_encrypted(channel.id, :CONTROL, 1))

      {:ok, _} =
        Lumia.Events.append(channel.id, actor, send_encrypted(channel.id, :APPLICATION, 1))

      {:ok, access} = Access.channel(a.id, channel.id)

      assert %V1.ChannelSnapshot{seq: 2, messages: [], encrypted_events: [e1, e2]} =
               Snapshots.channel(access)

      assert {:encrypted, %V1.EncryptedPayload{kind: :CONTROL}} = e1.payload
      assert {:encrypted, %V1.EncryptedPayload{kind: :APPLICATION}} = e2.payload
    end
  end

  test "a contact's roster change lands on the account log as ContactRosterUpdated" do
    %{identity: identity, device: phone, roster: v1} = Lumia.IdentityFixtures.bootstrap()
    {:ok, _} = Lumia.Identity.accept_roster(v1, :publish)

    {:ok, bound} =
      Lumia.Accounts.create_account(%{
        handle: "bound#{System.unique_integer([:positive])}",
        identity_id: identity.identity_id
      })

    other = Lumia.Fixtures.account("other")
    %{} = personal_dm(bound, other)
    assert Dms.contacts(bound.id) == [other.id]

    laptop = Lumia.IdentityFixtures.device("laptop")

    v2 =
      Lumia.IdentityFixtures.roster(identity, :root,
        version: 2,
        devices: [
          Lumia.IdentityFixtures.cert(identity, phone),
          Lumia.IdentityFixtures.cert(identity, laptop)
        ]
      )

    {:ok, _} = Lumia.Identity.accept_roster(v2, :publish)
    # Accepting the same version again is not news.
    {:ok, _} = Lumia.Identity.accept_roster(v2, :publish)

    events = Lumia.Accounts.events_after(other.id, 0)

    assert [%{payload: {:contact_roster_updated, %{identity_id: id, roster: ^v2}}}] =
             Enum.filter(events, &match?(%{payload: {:contact_roster_updated, _}}, &1))

    assert id == identity.identity_id
  end
end
