defmodule Lumia.DmsTest do
  use Lumia.DataCase, async: true

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Authz.Access
  alias Lumia.Dms
  alias Lumia.Snapshots
  alias Lumia.V1

  describe "open_personal/2" do
    test "creates once, then returns the existing channel for the pair in either order" do
      a = account("a")
      b = account("b")

      assert {:ok, channel, false} = Dms.open_personal(a.id, [b.id])
      assert channel.kind == "personal_dm"
      assert is_nil(channel.server_id)
      assert Dms.participants(channel.id) == Enum.sort([a.id, b.id])

      assert {:ok, %{id: same}, true} = Dms.open_personal(b.id, [a.id])
      assert same == channel.id
      # Including yourself in the list is fine.
      assert {:ok, %{id: ^same}, true} = Dms.open_personal(a.id, [a.id, b.id])

      assert "channel_added" in Accounts.event_kinds(a.id)
      assert "channel_added" in Accounts.event_kinds(b.id)
    end

    test "more than two participants is invalid for now" do
      [a, b, c] = for n <- ~w(a b c), do: account(n)
      assert {:error, :invalid_argument} = Dms.open_personal(a.id, [b.id, c.id])
      assert {:error, :invalid_argument} = Dms.open_personal(a.id, [])
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
    {:ok, _channel, false} = Dms.open_personal(bound.id, [other.id])
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
