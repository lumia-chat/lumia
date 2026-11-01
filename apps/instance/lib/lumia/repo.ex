defmodule Lumia.Repo do
  use Ecto.Repo,
    otp_app: :lumia,
    adapter: Ecto.Adapters.Postgres
end
