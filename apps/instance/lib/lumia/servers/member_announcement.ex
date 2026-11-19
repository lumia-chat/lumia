defmodule Lumia.Servers.MemberAnnouncement do
  @moduledoc """
  A member whose roles or title changed and whom the server's other
  members have not been told of yet (docs/specs/authorization.md section
  6.4): a row for each change, written in the change's transaction and
  deleted by `Lumia.Workers.MembersUpdated` once it has told them.
  """

  use Lumia.Schema

  schema "member_announcements" do
    field :server_id, Ecto.UUID
    field :account_id, Ecto.UUID
  end
end
