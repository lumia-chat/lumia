defmodule Lumia.Emoji do
  @moduledoc """
  Whether a string is one emoji, as a reaction must be (architecture 6.6):
  exactly one grapheme cluster, which starts with a codepoint of the emoji
  blocks, or is a keycap (a digit, `#` or `*` with U+20E3). Sequences joined
  with ZWJ, skin tones, flags and tag sequences are one grapheme each and
  pass. The blocks are wider than Unicode's Emoji property (a few symbols
  in them are not drawn as emoji); what matters here is that a reaction is
  a single picture and not a word.
  """

  # Leading codepoints of emoji: the pictographs and symbols blocks, the
  # older symbol blocks emoji were taken from, and the handful of others.
  @ranges [
    {0x1F000, 0x1FAFF},
    {0x2600, 0x27BF},
    {0x2300, 0x23FF},
    {0x2B00, 0x2BFF},
    {0x2190, 0x21FF},
    {0x25A0, 0x25FF},
    {0x2900, 0x297F},
    {0x3030, 0x3030},
    {0x303D, 0x303D},
    {0x3297, 0x3297},
    {0x3299, 0x3299},
    {0x00A9, 0x00A9},
    {0x00AE, 0x00AE},
    {0x203C, 0x203C},
    {0x2049, 0x2049},
    {0x2122, 0x2122},
    {0x2139, 0x2139},
    {0x24C2, 0x24C2}
  ]

  # The longest emoji sequences (families, flags of subdivisions) are about
  # 35 bytes; anything far longer is not one picture.
  @max_bytes 64

  @doc "Whether `text` is exactly one emoji."
  def single?(text)
      when is_binary(text) and byte_size(text) > 0 and byte_size(text) <= @max_bytes do
    case String.graphemes(text) do
      [_one] -> emoji_start?(String.to_charlist(text))
      _ -> false
    end
  end

  def single?(_), do: false

  defp emoji_start?([key | rest]) when key in ~c"0123456789#*" do
    0x20E3 in rest
  end

  defp emoji_start?([first | _]),
    do: Enum.any?(@ranges, fn {lo, hi} -> first >= lo and first <= hi end)

  defp emoji_start?([]), do: false
end
