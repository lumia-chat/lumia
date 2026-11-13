defmodule LumiaWeb.ChannelMembershipTest do
  @moduledoc """
  The endpoints of channel membership (architecture 6.8) and server DMs
  (architecture 6.4), over protobuf as the clients speak them.
  """

  use LumiaWeb.ConnCase, async: true

  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Proto
  alias Lumia.Servers
  alias Lumia.V1

  @proto "application/x-protobuf"

  defp authed(conn, account) do
    {:ok, _session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())

    conn
    |> put_req_header("authorization", "Bearer " <> tokens.access_token)
    |> put_req_header("accept", @proto)
    |> put_req_header("content-type", @proto)
  end

  defp call(conn, account, method, path, body \\ "") do
    conn |> authed(account) |> dispatch(@endpoint, method, path, body)
  end

  defp decode(conn, module), do: module.decode(conn.resp_body)

  setup %{conn: conn} do
    owner = account("owner")
    grace = account("grace")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    {:ok, _} = Servers.add_member(server.id, grace.id, owner.id)
    %{conn: conn, owner: owner, grace: grace, server: server, general: general}
  end

  test "a private channel is made, kept out of Browse, and entered by being added", ctx do
    made =
      call(
        ctx.conn,
        ctx.owner,
        :post,
        "/api/v1/channels",
        V1.CreateChannelRequest.encode(%V1.CreateChannelRequest{
          server_id: Proto.uuid(ctx.server.id),
          name: "secret",
          private: true
        })
      )

    assert made.status == 201
    assert %V1.Channel{private: true, is_default: false} = secret = decode(made, V1.Channel)
    secret_id = Proto.from_uuid(secret.channel_id)

    browsed = call(ctx.conn, ctx.grace, :get, "/api/v1/servers/#{ctx.server.id}/channels/browse")
    assert %V1.ListChannelsResponse{channels: []} = decode(browsed, V1.ListChannelsResponse)

    refused = call(ctx.conn, ctx.grace, :post, "/api/v1/channels/#{secret_id}/join")
    assert refused.status == 404

    added =
      call(
        ctx.conn,
        ctx.owner,
        :post,
        "/api/v1/channels/#{secret_id}/members",
        V1.AddChannelMembersRequest.encode(%V1.AddChannelMembersRequest{
          account_ids: [Proto.uuid(ctx.grace.id)]
        })
      )

    grace_id = Proto.uuid(ctx.grace.id)

    assert %V1.AddChannelMembersResponse{added_account_ids: [^grace_id]} =
             decode(added, V1.AddChannelMembersResponse)

    listed = call(ctx.conn, ctx.grace, :get, "/api/v1/channels/#{secret_id}/members")

    assert %V1.ListChannelMembersResponse{account_ids: ids} =
             decode(listed, V1.ListChannelMembersResponse)

    assert Enum.sort(ids) == Enum.sort([Proto.uuid(ctx.owner.id), grace_id])

    channels = call(ctx.conn, ctx.grace, :get, "/api/v1/servers/#{ctx.server.id}/channels")

    assert Enum.map(decode(channels, V1.ListChannelsResponse).channels, & &1.name) ==
             ["general", "secret"]

    assert call(
             ctx.conn,
             ctx.owner,
             :delete,
             "/api/v1/channels/#{secret_id}/members/#{ctx.grace.id}"
           ).status ==
             204

    assert call(ctx.conn, ctx.grace, :post, "/api/v1/channels/#{secret_id}/leave").status == 404
  end

  test "a public channel is browsed with its count, joined and left", ctx do
    {:ok, design} = Servers.create_channel(ctx.server.id, ctx.owner.id, "design")

    browsed = call(ctx.conn, ctx.grace, :get, "/api/v1/servers/#{ctx.server.id}/channels/browse")

    assert %V1.ListChannelsResponse{channels: [%V1.Channel{name: "design", member_count: 1}]} =
             decode(browsed, V1.ListChannelsResponse)

    joined = call(ctx.conn, ctx.grace, :post, "/api/v1/channels/#{design.id}/join")
    assert %V1.Channel{name: "design"} = decode(joined, V1.Channel)
    assert call(ctx.conn, ctx.grace, :post, "/api/v1/channels/#{design.id}/leave").status == 204
  end

  test "a member's switches, an invitation and its dismissal", ctx do
    {:ok, design} = Servers.create_channel(ctx.server.id, ctx.owner.id, "design")

    switched =
      call(
        ctx.conn,
        ctx.grace,
        :patch,
        "/api/v1/servers/#{ctx.server.id}/membership",
        V1.UpdateMembershipRequest.encode(%V1.UpdateMembershipRequest{
          channel_adds_by_invitation: true
        })
      )

    assert %V1.Member{channel_adds_by_invitation: true, server_dms_off: false} =
             decode(switched, V1.Member)

    invited =
      call(
        ctx.conn,
        ctx.owner,
        :post,
        "/api/v1/channels/#{design.id}/members",
        V1.AddChannelMembersRequest.encode(%V1.AddChannelMembersRequest{
          account_ids: [Proto.uuid(ctx.grace.id)]
        })
      )

    assert %V1.AddChannelMembersResponse{added_account_ids: [], invited_account_ids: [_]} =
             decode(invited, V1.AddChannelMembersResponse)

    snapshot = call(ctx.conn, ctx.grace, :get, "/api/v1/account/snapshot")

    assert %V1.AccountSnapshot{channel_invitations: [%V1.ChannelInvitation{}]} =
             decode(snapshot, V1.AccountSnapshot)

    assert call(ctx.conn, ctx.grace, :delete, "/api/v1/channel-invitations/#{design.id}").status ==
             204

    assert call(ctx.conn, ctx.grace, :delete, "/api/v1/channel-invitations/#{design.id}").status ==
             404
  end

  test "a server DM is opened, and refused when the other has them off", ctx do
    opened =
      call(
        ctx.conn,
        ctx.grace,
        :post,
        "/api/v1/servers/#{ctx.server.id}/dms",
        V1.OpenServerDmRequest.encode(%V1.OpenServerDmRequest{
          account_id: Proto.uuid(ctx.owner.id)
        })
      )

    assert %V1.Channel{kind: :SERVER_DM, participant_account_ids: [_, _]} =
             decode(opened, V1.Channel)

    {:ok, _} = Servers.update_membership(ctx.server.id, ctx.grace.id, %{server_dms_off: true})
    ada = account("ada")
    {:ok, _} = Servers.add_member(ctx.server.id, ada.id, ctx.owner.id)

    refused =
      call(
        ctx.conn,
        ada,
        :post,
        "/api/v1/servers/#{ctx.server.id}/dms",
        V1.OpenServerDmRequest.encode(%V1.OpenServerDmRequest{
          account_id: Proto.uuid(ctx.grace.id)
        })
      )

    assert refused.status == 400

    assert %V1.Error{code: :FAILED_PRECONDITION, details: %{"state" => "server_dms_off"}} =
             decode(refused, V1.Error)
  end

  test "the default channels are set with the server's settings", ctx do
    {:ok, design} = Servers.create_channel(ctx.server.id, ctx.owner.id, "design")

    set =
      call(
        ctx.conn,
        ctx.owner,
        :patch,
        "/api/v1/servers/#{ctx.server.id}",
        V1.UpdateServerRequest.encode(%V1.UpdateServerRequest{
          default_channels: %V1.ChannelIds{channel_ids: [Proto.uuid(design.id)]}
        })
      )

    assert set.status == 200
    assert Lumia.Repo.get!(Lumia.Events.Channel, design.id).is_default
    refute Lumia.Repo.get!(Lumia.Events.Channel, ctx.general.id).is_default

    refused =
      call(
        ctx.conn,
        ctx.grace,
        :patch,
        "/api/v1/servers/#{ctx.server.id}",
        V1.UpdateServerRequest.encode(%V1.UpdateServerRequest{
          default_channels: %V1.ChannelIds{channel_ids: []}
        })
      )

    assert refused.status == 403
  end
end
