defmodule Lumia.Workers.RetentionTest do
  use Lumia.DataCase, async: true
  use Oban.Testing, repo: Lumia.Repo

  import Lumia.Fixtures

  alias Lumia.Events
  alias Lumia.Events.EventPayload
  alias Lumia.Events.Message
  alias Lumia.Identity
  alias Lumia.Servers
  alias Lumia.V1
  alias Lumia.Workers.PurgeExpired
  alias Lumia.Workers.Retention

  defp age(channel_id, seq, days) do
    old = DateTime.add(DateTime.utc_now(), -days, :day)

    Repo.update_all(
      from(e in Lumia.Events.Event, where: e.channel_id == ^channel_id and e.seq <= ^seq),
      set: [created_at: old]
    )
  end

  describe "retention" do
    setup do
      owner = account("owner")
      {:ok, server} = Servers.create_server(owner.id, "Acme")
      {:ok, channel} = Servers.create_channel(server.id, owner.id, "general")
      actor = %{account_id: owner.id, device_id: nil}

      for i <- 1..5,
          do:
            {:ok, _} =
              Events.append(channel.id, actor, send_message(channel.id, "m#{i}"),
                broadcast: false
              )

      %{owner: owner, server: server, channel: channel, actor: actor}
    end

    test "purges payloads and messages older than the policy and appends PayloadsPurged", ctx do
      {:ok, _} = Servers.set_retention(ctx.server.id, ctx.owner.id, 30)
      age(ctx.channel.id, 3, 31)
      Phoenix.PubSub.subscribe(Lumia.PubSub, Events.topic(ctx.channel.id))

      assert {:ok, 1} = perform_job(Retention, %{})

      assert_receive {:event,
                      %V1.Event{
                        seq: 6,
                        payload: {:payloads_purged, %V1.PayloadsPurged{before_seq: 4}}
                      }, _}

      nulled =
        Repo.all(
          from p in EventPayload,
            where: p.channel_id == ^ctx.channel.id and is_nil(p.payload),
            select: p.seq
        )

      assert Enum.sort(nulled) == [1, 2, 3]

      bodies =
        Repo.all(
          from m in Message,
            where: m.channel_id == ^ctx.channel.id,
            order_by: m.seq,
            select: m.body
        )

      assert bodies == ["m4", "m5"]

      events = Events.events_after(ctx.channel.id, 0)

      assert Enum.map(events, &elem(&1.payload, 0)) == [
               :redacted,
               :redacted,
               :redacted,
               :message_created,
               :message_created,
               :payloads_purged
             ]

      assert %V1.Event{payload: {:redacted, %V1.Redacted{was: :REDACTED_MESSAGE}}} = hd(events)

      # Idempotent: nothing new is old enough, no second marker.
      assert {:ok, 0} = perform_job(Retention, %{})
      assert Repo.reload!(ctx.channel).last_seq == 6
    end

    test "servers without a policy are untouched", ctx do
      age(ctx.channel.id, 5, 400)
      assert {:ok, 0} = perform_job(Retention, %{})
      assert Repo.aggregate(from(p in EventPayload, where: is_nil(p.payload)), :count) == 0
    end

    test "setting retention needs MANAGE_SERVER and is audited", ctx do
      member = account("member")
      {:ok, _} = Servers.add_member(ctx.server.id, member.id, ctx.owner.id)
      assert {:error, :permission_denied} = Servers.set_retention(ctx.server.id, member.id, 7)
      assert {:ok, %{retention_days: 7}} = Servers.set_retention(ctx.server.id, ctx.owner.id, 7)

      assert Repo.exists?(
               from a in Lumia.Audit.Entry,
                 where: a.scope_id == ^ctx.server.id and a.action == "server.retention"
             )

      assert {:ok, %{retention_days: nil}} =
               Servers.set_retention(ctx.server.id, ctx.owner.id, nil)
    end
  end

  describe "purge expired" do
    test "removes expired challenges, bundles and reauth tokens, keeps live ones" do
      past = DateTime.add(DateTime.utc_now(), -1, :hour)
      future = DateTime.add(DateTime.utc_now(), 1, :hour)

      Repo.insert!(%Identity.UsedChallenge{nonce: "old", expires_at: past})
      Repo.insert!(%Identity.UsedChallenge{nonce: "new", expires_at: future})

      {:ok, bundle} =
        Identity.create_link_bundle(%V1.CreateLinkBundleRequest{
          ciphertext: "c",
          device_id: Lumia.Proto.uuid(Ecto.UUID.generate())
        })

      Repo.update_all(from(b in Identity.LinkBundle, where: b.code == ^bundle.code),
        set: [expires_at: past]
      )

      {:ok, session, _} = Lumia.Auth.create_session(account().id, Ecto.UUID.generate())
      {live_token, _} = Identity.issue_reauth(session)
      {dead_token, _} = Identity.issue_reauth(session)

      Repo.update_all(
        from(t in Identity.ReauthToken,
          where: t.token_hash == ^:crypto.hash(:sha256, dead_token)
        ),
        set: [expires_at: past]
      )

      assert {:ok, counts} = perform_job(PurgeExpired, %{})
      assert counts[Identity.UsedChallenge] == 1
      assert counts[Identity.LinkBundle] == 1
      assert counts[Identity.ReauthToken] == 1

      assert Repo.get(Identity.UsedChallenge, "new")
      assert {:ok, _} = Identity.verify_reauth(live_token)
    end
  end
end
