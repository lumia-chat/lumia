defmodule Lumia.Cards.Fetch do
  @moduledoc """
  The card fetcher (specs/media.md 6.3), for the instance's own requests
  to pages its members link. Every rule is about where it may connect and
  how much it reads:

    * `http` and `https` only, on ports 80 and 443, at most five
      redirects, each hop held to every rule;
    * the host is resolved here and the connection made to the address
      resolved, after `Lumia.Cards.Address` allowed every address the name
      has, so a name cannot pass the check and then lead somewhere inside;
    * five seconds for the whole of it, at most 1 MiB of a page (only
      HTML, and only until `</head>`), at most 5 MiB of a picture;
    * no cookies, and a user agent that says what it is.

  Mint directly, one connection per request: a pool keyed by host would
  connect by name and skip the address check.
  """

  alias Lumia.Cards.Address

  @timeout 5_000
  @max_redirects 5
  @max_page 1024 * 1024
  @max_image 5 * 1024 * 1024
  @html ["text/html", "application/xhtml+xml"]

  @doc """
  The page at `url`: `{:ok, final_url, html}`, or `{:error, reason}` for a
  refused address, scheme or port, a page that is not HTML, a failure, or
  the time running out.
  """
  def page(url) do
    deadline = System.monotonic_time(:millisecond) + @timeout

    with {:ok, final_url, type, body} <- get(url, @max_page, deadline, @max_redirects, :page) do
      if mime(type) in @html, do: {:ok, final_url, body}, else: {:error, :not_html}
    end
  end

  @doc "A picture at `url`, at most 5 MiB: `{:ok, bytes}` or `{:error, reason}`."
  def image(url) do
    deadline = System.monotonic_time(:millisecond) + @timeout

    case get(url, @max_image, deadline, @max_redirects, :image) do
      {:ok, _url, _type, body} -> {:ok, body}
      error -> error
    end
  end

  @doc "The user agent the fetcher names itself with."
  def user_agent,
    do: "LumiaBot/#{Application.spec(:lumia, :vsn)} (+https://lumia.chat/bot)"

  defp mime(nil), do: nil
  defp mime(type), do: type |> String.split(";") |> hd() |> String.trim() |> String.downcase()

  defp get(_url, _max, _deadline, redirects, _kind) when redirects < 0,
    do: {:error, :too_many_redirects}

  defp get(url, max, deadline, redirects, kind) do
    with {:ok, uri} <- target(url),
         {:ok, ip} <- resolve(uri.host),
         {:ok, status, headers, body} <- request(uri, ip, max, deadline, kind) do
      cond do
        status in [301, 302, 303, 307, 308] ->
          redirect(uri, header(headers, "location"), max, deadline, redirects, kind)

        status in 200..299 ->
          {:ok, URI.to_string(uri), header(headers, "content-type"), body}

        true ->
          {:error, {:status, status}}
      end
    end
  rescue
    _ -> {:error, :invalid_url}
  end

  defp redirect(_uri, nil, _max, _deadline, _redirects, _kind), do: {:error, :bad_redirect}

  defp redirect(uri, location, max, deadline, redirects, kind),
    do: get(URI.merge(uri, location) |> URI.to_string(), max, deadline, redirects - 1, kind)

  defp target(url) do
    case URI.new(url) do
      {:ok, %URI{scheme: scheme, host: host, port: port} = uri}
      when scheme in ["http", "https"] and is_binary(host) and host != "" and port in [80, 443] ->
        {:ok, uri}

      _ ->
        {:error, :refused}
    end
  end

  # Every address the name has must be allowed; the first is the one used.
  defp resolve(host) do
    case :inet.parse_address(String.to_charlist(host)) do
      {:ok, ip} ->
        if Address.allowed?(ip), do: {:ok, ip}, else: {:error, :refused}

      {:error, _} ->
        host |> String.to_charlist() |> lookup() |> vetted()
    end
  end

  defp vetted([]), do: {:error, :nxdomain}

  defp vetted(addresses) do
    if Enum.all?(addresses, &Address.allowed?/1),
      do: {:ok, hd(addresses)},
      else: {:error, :refused}
  end

  defp lookup(host) do
    Enum.flat_map([:inet, :inet6], fn family ->
      case :inet.getaddrs(host, family) do
        {:ok, list} -> list
        {:error, _} -> []
      end
    end)
  end

  defp request(uri, ip, max, deadline, kind) do
    scheme = String.to_existing_atom(uri.scheme)
    path = (uri.path || "/") <> if(uri.query, do: "?" <> uri.query, else: "")

    with {:ok, conn} <-
           Mint.HTTP.connect(scheme, ip, uri.port,
             hostname: uri.host,
             mode: :passive,
             protocols: [:http1],
             transport_opts: [timeout: remaining(deadline)]
           ) do
      accept = if kind == :page, do: "text/html,application/xhtml+xml", else: "image/*"

      headers = [
        {"user-agent", user_agent()},
        {"accept", accept},
        {"accept-language", "en"}
      ]

      result =
        case Mint.HTTP.request(conn, "GET", path, headers, nil) do
          {:ok, conn, ref} -> receive_response(conn, ref, max, deadline, kind, %{body: []})
          {:error, conn, reason} -> {:error, conn, reason}
        end

      case result do
        {:ok, conn, response} ->
          Mint.HTTP.close(conn)
          {:ok, response.status, response.headers, IO.iodata_to_binary(response.body)}

        {:error, conn, reason} ->
          Mint.HTTP.close(conn)
          {:error, reason}
      end
    end
  end

  defp receive_response(conn, ref, max, deadline, kind, acc) do
    case remaining(deadline) do
      0 ->
        {:error, conn, :timeout}

      wait ->
        case Mint.HTTP.recv(conn, 0, wait) do
          {:ok, conn, responses} ->
            responses
            |> collect(ref, acc, max, kind)
            |> continue(conn, ref, max, deadline, kind)

          {:error, conn, reason, _responses} ->
            {:error, conn, reason}
        end
    end
  end

  defp continue({:done, acc}, conn, _ref, _max, _deadline, _kind), do: {:ok, conn, acc}

  defp continue({:more, acc}, conn, ref, max, deadline, kind),
    do: receive_response(conn, ref, max, deadline, kind, acc)

  defp continue({:error, reason}, conn, _ref, _max, _deadline, _kind), do: {:error, conn, reason}

  defp collect([], _ref, acc, _max, _kind), do: {:more, acc}

  defp collect([{:status, ref, status} | rest], ref, acc, max, kind),
    do: collect(rest, ref, Map.put(acc, :status, status), max, kind)

  defp collect([{:headers, ref, headers} | rest], ref, acc, max, kind) do
    headers = Enum.map(headers, fn {k, v} -> {String.downcase(k), v} end)
    acc = Map.update(acc, :headers, headers, &(&1 ++ headers))

    # A redirect's or a refusal's body is never read.
    if acc[:status] in 200..299, do: collect(rest, ref, acc, max, kind), else: {:done, acc}
  end

  defp collect([{:data, ref, data} | rest], ref, acc, max, kind) do
    acc = %{acc | body: [acc.body, data]}
    size = IO.iodata_length(acc.body)

    cond do
      kind == :page and (size >= max or head_ended?(acc.body)) -> {:done, acc}
      kind == :image and size > max -> {:error, :too_large}
      true -> collect(rest, ref, acc, max, kind)
    end
  end

  defp collect([{:done, ref} | _], ref, acc, _max, _kind), do: {:done, acc}
  defp collect([_ | rest], ref, acc, max, kind), do: collect(rest, ref, acc, max, kind)

  defp head_ended?(body), do: Regex.match?(~r/<\/head/i, IO.iodata_to_binary(body))

  defp header(headers, name) do
    case List.keyfind(headers, name, 0) do
      {_, value} -> value
      nil -> nil
    end
  end

  defp remaining(deadline), do: max(deadline - System.monotonic_time(:millisecond), 0)
end
