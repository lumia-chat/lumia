defmodule Lumia.FakePages do
  @moduledoc """
  A stand-in for `Lumia.Cards.Fetch` (`:card_fetch`), which refuses to
  connect anywhere a test's own server could be: pages and pictures from
  canned answers below, and a count of the pages fetched.
  """

  @logo File.read!(Path.expand("fixtures/media/logo.png", __DIR__))

  def page("https://example.com/" <> _ = url, _deadline) do
    bump()

    {:ok, url,
     """
     <html><head>
     <meta property="og:title" content="An &amp; example">
     <meta property="og:description" content="What it is about">
     <meta property="og:site_name" content="Example">
     <meta property="og:image" content="/logo.png">
     </head><body>ignored</body></html>
     """}
  end

  def page(_url, _deadline) do
    bump()
    {:error, :refused}
  end

  def image("https://example.com/logo.png", _deadline), do: {:ok, @logo}
  def image(_url, _deadline), do: {:error, :refused}

  def count, do: :persistent_term.get({__MODULE__, :count}, 0)
  def reset, do: :persistent_term.put({__MODULE__, :count}, 0)
  defp bump, do: :persistent_term.put({__MODULE__, :count}, count() + 1)
end
