defmodule Lumia.Repo.Migrations.IndexPictureUrls do
  @moduledoc """
  A picture is served only while an account, a server or a live emoji
  names it (architecture 12), which `GET /avatars/{address}` asks on every
  fetch: the accounts' and servers' names are indexed for it, as the
  emoji's are.
  """

  use Ecto.Migration

  def change do
    create index(:accounts, [:avatar_url], where: "avatar_url IS NOT NULL")
    create index(:servers, [:icon_url], where: "icon_url IS NOT NULL")
  end
end
