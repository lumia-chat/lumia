defmodule LumiaWeb.Pictures do
  @moduledoc """
  A picture uploaded as a request's whole body, as chosen (PNG, JPEG, WebP
  or GIF, whatever the content type says): an account's
  (`PUT /account/avatar`) or a server's (`PUT /servers/{id}/icon`). The
  instance makes its own square of it (`Lumia.Media`).
  """

  alias LumiaWeb.ProtoConn

  @doc "The whole body, or `{:error, :too_large}` past the most `Lumia.Media` takes."
  def read(conn) do
    case Plug.Conn.read_body(conn, length: Lumia.Media.max_upload_bytes()) do
      {:ok, body, conn} -> {:ok, body, conn}
      {:more, _partial, _conn} -> {:error, :too_large}
      {:error, _reason} -> {:error, :invalid_argument}
    end
  end

  @doc """
  Renders a refusal of the picture itself (`details.field` `picture`, and
  why), or hands any other error back for the fallback.
  """
  def refused(conn, {:error, reason}) when reason in [:too_large, :unsupported, :invalid] do
    ProtoConn.render_error(conn, :invalid_argument, %{
      "field" => "picture",
      "reason" => Atom.to_string(reason)
    })
  end

  def refused(_conn, other), do: other
end
