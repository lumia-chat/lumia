defmodule LumiaWeb.Presence do
  @moduledoc """
  Which devices have a live gateway session, cluster-wide: a session
  tracks its device on join under the `devices` topic, and the push
  decision (`Lumia.Push.online?/1`) reads it.
  """

  use Phoenix.Presence, otp_app: :lumia, pubsub_server: Lumia.PubSub
end
