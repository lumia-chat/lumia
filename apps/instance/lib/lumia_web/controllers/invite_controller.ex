defmodule LumiaWeb.InviteController do
  @moduledoc "Invites (docs/protocol.md section 9, \"Invites\"; architecture 6.1 and 6.6)."

  use Phoenix.Controller, formats: []

  alias Lumia.Invites
  alias Lumia.Proto
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  # Public: what the trust card shows before the joiner agrees to anything.
  def show(conn, %{"code" => code}) do
    with {:ok, preview} <- Invites.preview(code) do
      ProtoConn.render(conn, %V1.InvitePreview{
        server: Proto.server(preview.server),
        channel: preview.channel && Proto.channel(preview.channel),
        member_count: preview.member_count,
        as_guest: preview.invite.as_guest,
        instance: LumiaWeb.InstanceController.info()
      })
    end
  end

  def index(conn, %{"server_id" => server_id}) do
    with {:ok, invites} <- Invites.list(server_id, conn.assigns.current_account_id) do
      ProtoConn.render(conn, %V1.ListInvitesResponse{invites: Enum.map(invites, &Proto.invite/1)})
    end
  end

  def create(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.CreateInviteRequest),
         server_id = Proto.from_uuid(req.server_id),
         {:ok, invite} <-
           Invites.create(server_id, conn.assigns.current_account_id, %{
             channel_id: Proto.from_uuid(req.channel_id),
             expires_at: Proto.from_timestamp(req.expires_at),
             max_uses: req.max_uses,
             as_guest: req.as_guest,
             guest_days: req.guest_days
           }) do
      ProtoConn.render(conn, 201, Proto.invite(invite))
    end
  end

  # Architecture 6.2: the invite as a mail to an address, one use, a week.
  def email(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.EmailInviteRequest),
         server_id = Proto.from_uuid(req.server_id),
         {:ok, invite} <-
           Invites.email(server_id, conn.assigns.current_account_id, req.email, %{
             as_guest: req.as_guest,
             guest_days: req.guest_days
           }) do
      ProtoConn.render(conn, 201, Proto.invite(invite))
    end
  end

  def accept(conn, %{"code" => code}) do
    with {:ok, member} <- Invites.accept(code, conn.assigns.current_account_id) do
      ProtoConn.render(conn, Proto.member(member))
    end
  end

  def delete(conn, %{"code" => code}) do
    with :ok <- Invites.delete(code, conn.assigns.current_account_id) do
      ProtoConn.no_content(conn)
    end
  end
end
