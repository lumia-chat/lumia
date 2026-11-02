defmodule Lumia.Relay.Provider do
  @moduledoc "A push platform behind the relay."

  @doc "Sends the notice to the device; `{:error, :gone}` means the token is dead."
  @callback deliver(token :: String.t(), instance :: String.t(), notice :: map()) ::
              :ok | {:error, :gone} | {:error, term()}

  defmodule Fake do
    @moduledoc "Records deliveries in the process named `:lumia_relay_fake` (tests, credential-less runs)."
    @behaviour Lumia.Relay.Provider

    @impl true
    def deliver(token, instance, notice) do
      case Process.whereis(:lumia_relay_fake) do
        nil -> :ok
        pid -> send(pid, {:delivered, token, instance, notice})
      end

      if String.starts_with?(token, "gone-"), do: {:error, :gone}, else: :ok
    end
  end
end
