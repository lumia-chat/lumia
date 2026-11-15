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
    * five seconds for the whole of it, the name's resolution and the
      page's picture included (`deadline/0`, passed to both), at most
      1 MiB of a page (only HTML, and only until its head has ended), at
      most 5 MiB of a picture;
    * no cookies, and a user agent that says what it is.

  Mint directly, one connection per request: a pool keyed by host would
  connect by name and skip the address check.
  """

  alias Lumia.Cards.Address
  alias Lumia.Cards.Head

  @timeout 5_000
  @max_redirects 5
  @max_page 1024 * 1024
  @max_image 5 * 1024 * 1024
  @html ["text/html", "application/xhtml+xml"]
  # How much more of a page arrives before the head is looked for again,
  # once a `</head` or `<body` that did not end it has been seen.
  @recheck_bytes 32 * 1024

  @doc "When a fetch begun now must be over: a card's page and its picture share it."
  def deadline, do: System.monotonic_time(:millisecond) + @timeout

  @doc """
  The page at `url`: `{:ok, final_url, html}`, or `{:error, reason}` for a
  refused address, scheme or port, a page that is not HTML, a failure, or
  the time running out by `deadline` (a monotonic time in milliseconds).
  """
  def page(url, deadline) do
    with {:ok, final_url, type, body} <- get(url, @max_page, deadline, @max_redirects, :page) do
      if mime(type) in @html, do: {:ok, final_url, body}, else: {:error, :not_html}
    end
  end

  @doc "A picture at `url`, at most 5 MiB, by `deadline`: `{:ok, bytes}` or `{:error, reason}`."
  def image(url, deadline) do
    case get(url, @max_image, deadline, @max_redirects, :image) do
      {:ok, _url, _type, body} -> {:ok, body}
      error -> error
    end
  end

  @doc """
  `url` parsed as a link the fetcher could follow, written as people write
  links: a character a URL may not hold as it is (a space, a `|`, a
  letter outside ASCII, as in `https://de.wikipedia.org/wiki/Bücher`) is
  percent-encoded first, as a browser and the core's fetcher do. `{:ok,
  %URI{}}` or `:error`.
  """
  def parse(url) when is_binary(url) do
    case URI.new(URI.encode(url, &(URI.char_unescaped?(&1) or &1 == ?%))) do
      {:ok, %URI{scheme: scheme, host: host} = uri}
      when scheme in ["http", "https"] and is_binary(host) and host != "" ->
        {:ok, uri}

      _ ->
        :error
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
         {:ok, ip} <- resolve(uri.host, deadline),
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
    case parse(url) do
      {:ok, %URI{port: port} = uri} when port in [80, 443] -> {:ok, uri}
      _ -> {:error, :refused}
    end
  end

  # Every address the name has must be allowed; the first is the one used.
  defp resolve(host, deadline) do
    case :inet.parse_address(String.to_charlist(host)) do
      {:ok, ip} ->
        if Address.allowed?(ip), do: {:ok, ip}, else: {:error, :refused}

      {:error, _} ->
        host |> String.to_charlist() |> lookup(deadline) |> vetted()
    end
  end

  defp vetted([]), do: {:error, :nxdomain}

  defp vetted(addresses) do
    if Enum.all?(addresses, &Address.allowed?/1),
      do: {:ok, hd(addresses)},
      else: {:error, :refused}
  end

  # Within the deadline: a name whose servers do not answer runs it out,
  # and its fetch gets nothing.
  defp lookup(host, deadline) do
    Enum.flat_map([:inet, :inet6], fn family ->
      case :inet.getaddrs(host, family, max(remaining(deadline), 1)) do
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
          {:ok, conn, ref} -> receive_response(conn, ref, max, deadline, kind, new_acc())
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

  defp new_acc, do: %{body: [], size: 0, tail: "", checked: nil, pending: false}

  defp collect([], _ref, acc, _max, _kind), do: {:more, acc}

  defp collect([{:status, ref, status} | rest], ref, acc, max, kind),
    do: collect(rest, ref, Map.put(acc, :status, status), max, kind)

  # An informational answer (100 Continue, 103 Early Hints) comes before
  # the real one: its headers are dropped and the next status awaited.
  defp collect([{:headers, ref, _headers} | rest], ref, %{status: status} = acc, max, kind)
       when status in 100..199,
       do: collect(rest, ref, Map.delete(acc, :status), max, kind)

  defp collect([{:headers, ref, headers} | rest], ref, acc, max, kind) do
    headers = Enum.map(headers, fn {k, v} -> {String.downcase(k), v} end)
    acc = Map.update(acc, :headers, headers, &(&1 ++ headers))

    # A redirect's or a refusal's body is never read.
    if acc[:status] in 200..299, do: collect(rest, ref, acc, max, kind), else: {:done, acc}
  end

  defp collect([{:data, ref, data} | rest], ref, acc, max, kind) do
    acc = %{acc | body: [acc.body, data], size: acc.size + byte_size(data)}

    case kind do
      :page ->
        {ended, acc} = head_ended(acc, data)
        if ended or acc.size >= max, do: {:done, acc}, else: collect(rest, ref, acc, max, kind)

      :image ->
        if acc.size > max, do: {:error, :too_large}, else: collect(rest, ref, acc, max, kind)
    end
  end

  defp collect([{:done, ref} | _], ref, acc, _max, _kind), do: {:done, acc}
  defp collect([_ | rest], ref, acc, max, kind), do: collect(rest, ref, acc, max, kind)

  # Whether the page's head is over, by the tokenizer that reads it: a
  # `</head` inside a comment or a script does not end it. The tokenizer
  # runs only once the new bytes (with the end of the last ones, for a tag
  # split between two) have held `</head` or `<body`, and, after a run
  # that found the head going on, not again until #{@recheck_bytes} more
  # bytes have come, so a page cannot make it run at every packet.
  defp head_ended(acc, data) do
    joined = acc.tail <> data
    seen = acc.pending or Regex.match?(~r/<\/head|<body/i, joined)
    due = seen and (acc.checked == nil or acc.size - acc.checked >= @recheck_bytes)
    ended = due and Head.ended?(IO.iodata_to_binary(acc.body))
    tail = binary_part(joined, max(byte_size(joined) - 5, 0), min(byte_size(joined), 5))

    {ended,
     %{
       acc
       | tail: tail,
         checked: if(due, do: acc.size, else: acc.checked),
         pending: seen and not due
     }}
  end

  defp header(headers, name) do
    case List.keyfind(headers, name, 0) do
      {_, value} -> value
      nil -> nil
    end
  end

  defp remaining(deadline), do: max(deadline - System.monotonic_time(:millisecond), 0)
end
