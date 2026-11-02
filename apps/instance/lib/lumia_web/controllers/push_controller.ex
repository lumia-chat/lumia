defmodule LumiaWeb.PushController do
  @moduledoc "`PUT` and `DELETE /push/subscription` (architecture 15.5)."

  use LumiaWeb, :controller

  action_fallback LumiaWeb.FallbackController

  alias Lumia.Push
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  def put(conn, _params) do
    with {:ok, sub} <- ProtoConn.decode(conn, V1.PushSubscription),
         :ok <- Push.put(conn.assigns.current_account_id, conn.assigns.current_device_id, sub) do
      ProtoConn.no_content(conn)
    end
  end

  def delete(conn, params) do
    with {:ok, kind} <- kind(params["kind"]) do
      :ok = Push.delete(conn.assigns.current_device_id, kind)
      ProtoConn.no_content(conn)
    end
  end

  defp kind(nil), do: {:error, :invalid_argument}

  defp kind(name) do
    name = String.downcase(name)
    if name in Lumia.Push.Subscription.kinds(), do: {:ok, name}, else: {:error, :invalid_argument}
  end
end
