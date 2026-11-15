defmodule Lumia.Cards.Head do
  @moduledoc """
  A card from a page's `<head>` (specs/media.md 6.2): Open Graph first,
  then Twitter's tags, then `<title>` and `<meta name="description">`,
  each field falling back on its own. A port of the core's
  `lumia_core::cards::parse_head`, step for step, and checked against the
  same vectors (`proto/vectors/cards`), since the instance builds server
  cards and the core personal ones.

  A small tokenizer, not an HTML parser: it skips comments, declarations
  and the raw text of scripts and styles, reads `<meta>`, `<base>` and
  `<title>`, and stops at `</head>` or `<body>`.
  """

  @max_html 1024 * 1024
  @max_title 300
  @max_description 1000
  @max_site_name 300
  @raw ~w(script style template textarea xmp iframe noembed noframes)

  @doc """
  The card `html` gives for the page at `page_url`: a map with `:url`,
  `:title`, `:description`, `:site_name`, `:color`, `:image_url`,
  `:image_width`, `:image_height` and `:video`, or nil when it has no title.
  """
  def parse(html, page_url) when is_binary(html), do: html |> read() |> build(page_url)

  @doc """
  Whether the head of `html`, the start of a page, is over as the
  tokenizer reads it (a `</head>` or `<body>` outside comments and
  scripts), for the fetcher to stop reading there.
  """
  def ended?(html) when is_binary(html), do: read(html).ended

  defp read(html) do
    html
    |> binary_part(0, min(byte_size(html), @max_html))
    |> String.replace_invalid()
    |> scan()
  end

  defp build(head, page_url) do
    title =
      text(head, "og:title", @max_title) || text(head, "twitter:title", @max_title) ||
        (head.title && non_empty(clean(head.title, @max_title)))

    if title do
      base = base_url(page_url, head.base)
      {image_url, width, height} = image(head, base)

      %{
        url: page_url,
        title: title,
        description: description(head),
        site_name: text(head, "og:site_name", @max_site_name) || "",
        color: Enum.find_value(head.metas, "", fn {k, v} -> k == "theme-color" && color(v) end),
        image_url: image_url,
        image_width: width,
        image_height: height,
        video: Enum.any?(head.metas, &video?/1)
      }
    end
  end

  defp description(head) do
    text(head, "og:description", @max_description) ||
      text(head, "twitter:description", @max_description) ||
      text(head, "description", @max_description) || ""
  end

  defp video?({"og:video" <> _, _}), do: true

  defp video?({"og:type", value}),
    do: value |> String.trim() |> String.downcase() |> String.starts_with?("video")

  defp video?(_), do: false

  defp text(head, key, max) do
    Enum.find_value(head.metas, fn {k, v} -> k == key && non_empty(clean(v, max)) end)
  end

  defp non_empty(""), do: nil
  defp non_empty(text), do: text

  defp base_url(page_url, base) do
    case URI.new(String.trim(page_url)) do
      {:ok, %URI{scheme: scheme} = page} when is_binary(scheme) ->
        if base, do: join(page, base) || page, else: page

      _ ->
        nil
    end
  end

  defp join(base, href) do
    URI.merge(base, String.trim(href))
  rescue
    _ -> nil
  end

  # The first og:image (its https secure_url preferred), else the first
  # Twitter image that is a web address.
  defp image(head, base) do
    og_picked(og_image(head.metas), base) ||
      Enum.find_value(head.metas, {"", 0, 0}, fn {k, v} ->
        url = k in ["twitter:image", "twitter:image:src"] && web(base, v)
        url && {url, 0, 0}
      end)
  end

  defp og_picked(nil, _base), do: nil

  defp og_picked(og, base) do
    secure = og.secure && web(base, og.secure)
    secure = if secure && String.starts_with?(secure, "https:"), do: secure
    url = secure || (og.url && web(base, og.url))
    url && {url, og.width, og.height}
  end

  # The first og:image and what follows it up to the next one.
  defp og_image(metas) do
    Enum.reduce_while(metas, nil, fn {key, value}, image ->
      value = String.trim(value)

      case {key, image} do
        {k, nil} when k in ["og:image", "og:image:url"] ->
          {:cont, %{url: value, secure: nil, width: 0, height: 0}}

        {k, %{}} when k in ["og:image", "og:image:url"] ->
          {:halt, image}

        {"og:image:secure_url", %{secure: nil}} ->
          {:cont, %{image | secure: value}}

        {"og:image:width", %{width: 0}} ->
          {:cont, %{image | width: dimension(value)}}

        {"og:image:height", %{height: 0}} ->
          {:cont, %{image | height: dimension(value)}}

        _ ->
          {:cont, image}
      end
    end)
  end

  defp dimension(value) do
    case Integer.parse(value) do
      {n, ""} when n >= 0 and n <= 0xFFFFFFFF -> n
      _ -> 0
    end
  end

  defp web(base, href) do
    href = String.trim(href)

    with true <- href != "",
         {:ok, uri} <- resolve(base, href),
         true <- uri.scheme in ["http", "https"] and uri.host not in [nil, ""] do
      URI.to_string(uri)
    else
      _ -> nil
    end
  end

  defp resolve(nil, href), do: URI.new(href)

  defp resolve(base, href) do
    case URI.new(href) do
      {:ok, %URI{scheme: scheme} = uri} when is_binary(scheme) -> {:ok, uri}
      _ -> {:ok, URI.merge(base, href)}
    end
  rescue
    _ -> :error
  end

  defp color(value) do
    with "#" <> hex <- String.trim(value),
         true <- hex =~ ~r/\A[0-9a-fA-F]*\z/ do
      case String.downcase(hex) do
        <<r, g, b>> -> <<?#, r, r, g, g, b, b>>
        <<_::binary-size(6)>> = long -> "#" <> long
        _ -> nil
      end
    else
      _ -> nil
    end
  end

  # Unicode whitespace collapsed, control characters dropped, then cut to
  # `max` graphemes with an ellipsis.
  @whitespace ~r/[\s\x{85}\x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}]+/u
  defp clean(text, max) do
    joined =
      text
      |> String.split(@whitespace, trim: true)
      |> Enum.join(" ")
      |> String.replace(~r/\p{Cc}/u, "")

    graphemes = String.graphemes(joined)

    if length(graphemes) > max,
      do: (graphemes |> Enum.take(max - 1) |> Enum.join() |> String.trim_trailing()) <> "…",
      else: joined
  end

  # ---- The tokenizer -----------------------------------------------------------

  # `lower` is the text in ASCII lower case, made once: the same offsets,
  # so a closing tag is found in any case without copying the rest of the
  # page at every search.
  defp scan(text),
    do:
      scan(text, 0, %{
        title: nil,
        base: nil,
        metas: [],
        ended: false,
        lower: String.downcase(text, :ascii)
      })

  defp scan(text, i, head) do
    case :binary.match(text, "<", scope: {i, byte_size(text) - i}) do
      :nomatch -> finish(head)
      {at, _} -> tag(text, at, binary_part(text, at, min(4, byte_size(text) - at)), head)
    end
  end

  defp finish(head), do: %{head | metas: Enum.reverse(head.metas)}

  defp tag(text, at, "<!--", head), do: scan(text, after_match(text, at + 4, "-->"), head)
  defp tag(text, at, "<!" <> _, head), do: scan(text, after_match(text, at, ">"), head)
  defp tag(text, at, "<?" <> _, head), do: scan(text, after_match(text, at, ">"), head)

  defp tag(text, at, "</" <> _, head) do
    case tag_name(text, at + 2) do
      {"", _} -> scan(text, at + 2, head)
      {"head", _} -> finish(%{head | ended: true})
      {_, after_name} -> scan(text, after_match(text, after_name, ">"), head)
    end
  end

  defp tag(text, at, _, head) do
    case tag_name(text, at + 1) do
      {"", _} ->
        scan(text, at + 1, head)

      {name, after_name} ->
        {attrs, i} = attributes(text, after_name, [])
        element(name, attrs, text, i, head)
    end
  end

  defp element("body", _attrs, _text, _i, head), do: finish(%{head | ended: true})
  defp element("meta", attrs, text, i, head), do: scan(text, i, meta(attrs, head))

  defp element("base", attrs, text, i, %{base: nil} = head),
    do: scan(text, i, %{head | base: attr(attrs, "href")})

  defp element("title", _attrs, text, i, head) do
    {raw, next} = raw_text(text, head.lower, i, "title")
    head = if head.title == nil, do: %{head | title: decode(raw)}, else: head
    scan(text, next, head)
  end

  defp element(name, _attrs, text, i, head) when name in @raw,
    do: scan(text, elem(raw_text(text, head.lower, i, name), 1), head)

  defp element(_name, _attrs, text, i, head), do: scan(text, i, head)

  defp meta(attrs, head) do
    case attr(attrs, "content") do
      nil ->
        head

      content ->
        property = key(attrs, "property")
        name = key(attrs, "name")
        metas = if property, do: [{property, content} | head.metas], else: head.metas
        metas = if name && name != property, do: [{name, content} | metas], else: metas
        %{head | metas: metas}
    end
  end

  defp key(attrs, name) do
    case attr(attrs, name) do
      nil -> nil
      value -> non_empty(value |> String.trim() |> String.downcase(:ascii))
    end
  end

  defp attr(attrs, name), do: Enum.find_value(attrs, fn {n, v} -> n == name && v end)

  # Where `needle` ends, from `from`; the end of the text when absent. The
  # needles searched in the text as written have no letters; a closing tag
  # is searched in the lower-case copy.
  defp after_match(text, from, needle) do
    case find(text, from, needle) do
      nil -> byte_size(text)
      at -> at + byte_size(needle)
    end
  end

  defp find(text, from, needle) when from <= byte_size(text) do
    case :binary.match(text, needle, scope: {from, byte_size(text) - from}) do
      {at, _} -> at
      :nomatch -> nil
    end
  end

  defp find(_text, _from, _needle), do: nil

  defp tag_name(text, at) do
    case text do
      <<_::binary-size(^at), first, _::binary>> when first in ?a..?z or first in ?A..?Z ->
        end_at = skip(text, at, &(not ws?(&1) and &1 not in [?/, ?>]))
        {String.downcase(binary_part(text, at, end_at - at), :ascii), end_at}

      _ ->
        {"", at}
    end
  end

  defp attributes(text, at, attrs) do
    at = skip(text, at, &(ws?(&1) or &1 == ?/))

    case byte_at(text, at) do
      nil ->
        {Enum.reverse(attrs), byte_size(text)}

      ?> ->
        {Enum.reverse(attrs), at + 1}

      _ ->
        name_end = skip(text, at + 1, &(not ws?(&1) and &1 not in [?/, ?>, ?=]))
        name = String.downcase(binary_part(text, at, name_end - at), :ascii)
        at = skip(text, name_end, &ws?/1)
        {value, at} = value(text, at)
        attributes(text, at, [{name, value} | attrs])
    end
  end

  defp value(text, at) do
    if byte_at(text, at) == ?= do
      at = skip(text, at + 1, &ws?/1)

      case byte_at(text, at) do
        quote when quote in [?", ?'] ->
          to = closing(text, at + 1, quote)
          {decode(binary_part(text, at + 1, to - at - 1)), min(to + 1, byte_size(text))}

        _ ->
          to = skip(text, at, &(not ws?(&1) and &1 != ?>))
          {decode(binary_part(text, at, to - at)), to}
      end
    else
      {"", at}
    end
  end

  defp closing(text, from, quote) do
    case :binary.match(text, <<quote>>, scope: {from, byte_size(text) - from}) do
      {to, _} -> to
      :nomatch -> byte_size(text)
    end
  end

  defp raw_text(text, lower, at, name), do: raw_text(text, lower, at, at, "</" <> name)

  defp raw_text(text, lower, at, from, close) do
    case find(lower, from, close) do
      nil ->
        {binary_part(text, at, byte_size(text) - at), byte_size(text)}

      end_at ->
        after_close = byte_at(text, end_at + byte_size(close))

        if after_close == nil or ws?(after_close) or after_close in [?/, ?>],
          do: {binary_part(text, at, end_at - at), after_match(text, end_at, ">")},
          else: raw_text(text, lower, at, end_at + byte_size(close), close)
    end
  end

  defp skip(text, at, keep?) do
    case byte_at(text, at) do
      nil -> at
      byte -> if keep?.(byte), do: skip(text, at + 1, keep?), else: at
    end
  end

  defp byte_at(text, at) when at < byte_size(text), do: :binary.at(text, at)
  defp byte_at(_text, _at), do: nil

  defp ws?(byte), do: byte in [?\s, ?\t, ?\n, ?\r, ?\f]

  # ---- Character references ----------------------------------------------------

  # A reference ends with `;` within 40 bytes; anything else stays as written.
  defp decode(text) do
    if String.contains?(text, "&"), do: decode(text, []), else: text
  end

  defp decode(text, acc) do
    case :binary.split(text, "&") do
      [rest] ->
        IO.iodata_to_binary(Enum.reverse([rest | acc]))

      [before, rest] ->
        case reference("&" <> rest) do
          {char, len} ->
            decode(binary_part(rest, len - 1, byte_size(rest) - len + 1), [char, before | acc])

          nil ->
            decode(rest, ["&", before | acc])
        end
    end
  end

  defp reference(text) do
    window = binary_part(text, 0, min(byte_size(text), 40))

    with {semi, _} <- :binary.match(window, ";"),
         body = binary_part(text, 1, semi - 1),
         char when is_binary(char) <- referenced(body) do
      {char, semi + 1}
    else
      _ -> nil
    end
  end

  defp referenced("#" <> number) do
    code =
      case number do
        <<x, hex::binary>> when x in [?x, ?X] and hex != "" ->
          if hex =~ ~r/\A[0-9a-fA-F]+\z/, do: String.to_integer(hex, 16)

        digits when digits != "" ->
          if digits =~ ~r/\A[0-9]+\z/, do: String.to_integer(digits)

        _ ->
          nil
      end

    code && numeric(code)
  end

  defp referenced(name), do: Map.get(named(), name)

  @windows_1252 ~w(€ \u0081 ‚ ƒ „ … † ‡ ˆ ‰ Š ‹ Œ \u008D Ž \u008F \u0090 ‘ ’ “ ” • – — ˜ ™ š › œ \u009D ž Ÿ)

  defp numeric(code) when code in 0x80..0x9F, do: Enum.at(@windows_1252, code - 0x80)

  defp numeric(code) when code in 1..0x10FFFF and code not in 0xD800..0xDFFF, do: <<code::utf8>>
  defp numeric(_code), do: "�"

  defp named do
    %{
      "amp" => "&",
      "lt" => "<",
      "gt" => ">",
      "quot" => "\"",
      "apos" => "'",
      "nbsp" => "\u00A0",
      "iexcl" => "¡",
      "cent" => "¢",
      "pound" => "£",
      "curren" => "¤",
      "yen" => "¥",
      "brvbar" => "¦",
      "sect" => "§",
      "uml" => "¨",
      "copy" => "©",
      "ordf" => "ª",
      "laquo" => "«",
      "not" => "¬",
      "shy" => "\u00AD",
      "reg" => "®",
      "macr" => "¯",
      "deg" => "°",
      "plusmn" => "±",
      "sup2" => "²",
      "sup3" => "³",
      "acute" => "´",
      "micro" => "µ",
      "para" => "¶",
      "middot" => "·",
      "cedil" => "¸",
      "sup1" => "¹",
      "ordm" => "º",
      "raquo" => "»",
      "frac14" => "¼",
      "frac12" => "½",
      "frac34" => "¾",
      "iquest" => "¿",
      "Agrave" => "À",
      "Aacute" => "Á",
      "Acirc" => "Â",
      "Atilde" => "Ã",
      "Auml" => "Ä",
      "Aring" => "Å",
      "AElig" => "Æ",
      "Ccedil" => "Ç",
      "Egrave" => "È",
      "Eacute" => "É",
      "Ecirc" => "Ê",
      "Euml" => "Ë",
      "Igrave" => "Ì",
      "Iacute" => "Í",
      "Icirc" => "Î",
      "Iuml" => "Ï",
      "ETH" => "Ð",
      "Ntilde" => "Ñ",
      "Ograve" => "Ò",
      "Oacute" => "Ó",
      "Ocirc" => "Ô",
      "Otilde" => "Õ",
      "Ouml" => "Ö",
      "times" => "×",
      "Oslash" => "Ø",
      "Ugrave" => "Ù",
      "Uacute" => "Ú",
      "Ucirc" => "Û",
      "Uuml" => "Ü",
      "Yacute" => "Ý",
      "THORN" => "Þ",
      "szlig" => "ß",
      "agrave" => "à",
      "aacute" => "á",
      "acirc" => "â",
      "atilde" => "ã",
      "auml" => "ä",
      "aring" => "å",
      "aelig" => "æ",
      "ccedil" => "ç",
      "egrave" => "è",
      "eacute" => "é",
      "ecirc" => "ê",
      "euml" => "ë",
      "igrave" => "ì",
      "iacute" => "í",
      "icirc" => "î",
      "iuml" => "ï",
      "eth" => "ð",
      "ntilde" => "ñ",
      "ograve" => "ò",
      "oacute" => "ó",
      "ocirc" => "ô",
      "otilde" => "õ",
      "ouml" => "ö",
      "divide" => "÷",
      "oslash" => "ø",
      "ugrave" => "ù",
      "uacute" => "ú",
      "ucirc" => "û",
      "uuml" => "ü",
      "yacute" => "ý",
      "thorn" => "þ",
      "yuml" => "ÿ",
      "OElig" => "Œ",
      "oelig" => "œ",
      "Scaron" => "Š",
      "scaron" => "š",
      "Yuml" => "Ÿ",
      "fnof" => "ƒ",
      "circ" => "ˆ",
      "tilde" => "˜",
      "ensp" => "\u2002",
      "emsp" => "\u2003",
      "thinsp" => "\u2009",
      "zwnj" => "\u200C",
      "zwj" => "\u200D",
      "lrm" => "\u200E",
      "rlm" => "\u200F",
      "ndash" => "–",
      "mdash" => "—",
      "lsquo" => "‘",
      "rsquo" => "’",
      "sbquo" => "‚",
      "ldquo" => "“",
      "rdquo" => "”",
      "bdquo" => "„",
      "dagger" => "†",
      "Dagger" => "‡",
      "bull" => "•",
      "hellip" => "…",
      "permil" => "‰",
      "lsaquo" => "‹",
      "rsaquo" => "›",
      "euro" => "€",
      "trade" => "™"
    }
  end
end
