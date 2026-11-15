defmodule Lumia.Cards.Head do
  @moduledoc """
  A card from a page's `<head>` (specs/media.md 6.2): Open Graph first,
  then Twitter's tags, then `<title>` and `<meta name="description">`. The
  same rules as the core's `lumia_core::cards::parse_head`, checked against
  the same vectors. A tolerant scan of tags, not an HTML parser: it reads
  only `<meta>` and `<title>` and stops at `</head>`.
  """

  @max_html 1024 * 1024
  @max_title 300
  @max_description 1000

  @doc """
  The card `html` gives for the page at `page_url`: a map with `:url`,
  `:title`, `:description`, `:site_name`, `:color`, `:image_url`,
  `:image_width`, `:image_height` and `:video`, or nil when it has no title.
  """
  def parse(html, page_url) when is_binary(html) do
    head = head_of(html)
    meta = metas(head)

    title =
      first([meta["og:title"], meta["twitter:title"], title_of(head)])
      |> clean(@max_title)

    if title == "" do
      nil
    else
      %{
        url: page_url,
        title: title,
        description:
          first([meta["og:description"], meta["twitter:description"], meta["description"]])
          |> clean(@max_description),
        site_name: clean(meta["og:site_name"] || "", @max_title),
        color: color(meta["theme-color"]),
        image_url: image(meta, page_url),
        image_width: int(meta["og:image:width"]),
        image_height: int(meta["og:image:height"]),
        video:
          Enum.any?(Map.keys(meta), &String.starts_with?(&1, "og:video")) or
            String.starts_with?(meta["og:type"] || "", "video")
      }
    end
  end

  defp head_of(html) do
    html = binary_part(html, 0, min(byte_size(html), @max_html))

    case Regex.run(~r/<\/head/i, html, return: :index) do
      [{at, _}] -> binary_part(html, 0, at)
      nil -> html
    end
  end

  # The first value of each meta name or property, names in lowercase.
  defp metas(head) do
    ~r/<meta\b([^>]*)>/is
    |> Regex.scan(head, capture: :all_but_first)
    |> Enum.reduce(%{}, fn [attrs], acc ->
      attrs = attributes(attrs)
      key = attrs["property"] || attrs["name"]
      content = attrs["content"]

      if key && content && not Map.has_key?(acc, String.downcase(key)),
        do: Map.put(acc, String.downcase(key), decode(content)),
        else: acc
    end)
  end

  defp attributes(text) do
    ~r/([a-zA-Z_:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))/s
    |> Regex.scan(text, capture: :all_but_first)
    |> Enum.reduce(%{}, fn [name | values], acc ->
      Map.put_new(acc, String.downcase(name), Enum.find(values, "", &(&1 != "")))
    end)
  end

  defp title_of(head) do
    case Regex.run(~r/<title\b[^>]*>(.*?)<\/title>/is, head, capture: :all_but_first) do
      [title] -> decode(title)
      _ -> nil
    end
  end

  defp first(values), do: Enum.find(values, "", &(is_binary(&1) and String.trim(&1) != ""))

  # Whitespace collapsed, then cut to `max` characters with an ellipsis.
  defp clean(text, max) do
    text = text |> String.split() |> Enum.join(" ")

    if String.length(text) > max,
      do: String.slice(text, 0, max - 1) |> String.trim_trailing() |> Kernel.<>("…"),
      else: text
  end

  defp color(nil), do: ""

  defp color(value) do
    case String.trim(value) |> String.downcase() do
      "#" <> <<r, g, b>> = short when r in ?0..?9 or r in ?a..?f ->
        if hex?(short), do: "#" <> <<r, r, g, g, b, b>>, else: ""

      "#" <> <<_::binary-size(6)>> = long ->
        if hex?(long), do: long, else: ""

      _ ->
        ""
    end
  end

  defp hex?("#" <> digits), do: digits =~ ~r/\A[0-9a-f]+\z/

  defp image(meta, page_url) do
    secure = meta["og:image:secure_url"]

    candidate =
      if is_binary(secure) and String.starts_with?(String.trim(secure), "https://"),
        do: secure,
        else: first([meta["og:image"], meta["og:image:url"], meta["twitter:image"]])

    with true <- candidate != "",
         %URI{scheme: scheme} = uri when scheme in ["http", "https"] <-
           URI.merge(page_url, String.trim(candidate)) do
      URI.to_string(uri)
    else
      _ -> ""
    end
  rescue
    _ -> ""
  end

  defp int(nil), do: 0

  defp int(text) do
    case Integer.parse(String.trim(text)) do
      {n, ""} when n > 0 and n < 100_000 -> n
      _ -> 0
    end
  end

  @named %{
    "amp" => "&",
    "lt" => "<",
    "gt" => ">",
    "quot" => "\"",
    "apos" => "'",
    "nbsp" => " ",
    "#39" => "'"
  }

  defp decode(text) do
    Regex.replace(~r/&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);/, text, fn whole, entity ->
      cond do
        Map.has_key?(@named, entity) ->
          @named[entity]

        String.starts_with?(entity, "#x") ->
          codepoint(Integer.parse(String.slice(entity, 2..-1//1), 16), whole)

        String.starts_with?(entity, "#") ->
          codepoint(Integer.parse(String.slice(entity, 1..-1//1)), whole)

        true ->
          whole
      end
    end)
  end

  defp codepoint({n, ""}, _whole) when n in 1..0x10FFFF and n not in 0xD800..0xDFFF,
    do: <<n::utf8>>

  defp codepoint(_, whole), do: whole
end
