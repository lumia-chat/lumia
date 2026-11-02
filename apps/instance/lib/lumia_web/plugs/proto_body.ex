defmodule LumiaWeb.Plugs.ProtoBody do
  @moduledoc """
  A `Plug.Parsers` parser for the API's two encodings. It reads the raw
  body for `application/json` and `application/x-protobuf` and keeps it
  in `conn.private.lumia_raw_body` so controllers can decode it into the
  right `Lumia.V1` struct. Params stay empty; the API does not use them.
  """

  @behaviour Plug.Parsers

  @impl Plug.Parsers
  def init(opts), do: opts

  @impl Plug.Parsers
  def parse(conn, "application", subtype, _headers, opts)
      when subtype in ["json", "x-protobuf"] do
    case Plug.Conn.read_body(conn, opts) do
      {:ok, body, conn} ->
        {:ok, %{}, Plug.Conn.put_private(conn, :lumia_raw_body, {subtype, body})}

      {:more, _, conn} ->
        {:error, :too_large, conn}

      {:error, :timeout} ->
        raise Plug.TimeoutError

      {:error, _} ->
        raise Plug.BadRequestError
    end
  end

  def parse(conn, _type, _subtype, _headers, _opts), do: {:next, conn}
end
