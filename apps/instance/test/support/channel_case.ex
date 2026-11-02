defmodule LumiaWeb.ChannelCase do
  @moduledoc """
  Test case for the gateway: Phoenix.ChannelTest against the endpoint with
  the SQL sandbox. Note that ChannelTest bypasses the serializer, so pushes
  are asserted as the raw payloads the channel hands to `push/3`:
  `%Lumia.V1.ServerFrame{}` or `{:event_bytes, binary}`.
  """

  use ExUnit.CaseTemplate

  using do
    quote do
      import Phoenix.ChannelTest
      import LumiaWeb.ChannelCase

      @endpoint LumiaWeb.Endpoint
    end
  end

  setup tags do
    Lumia.DataCase.setup_sandbox(tags)
    :ok
  end
end
