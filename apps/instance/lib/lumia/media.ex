defmodule Lumia.Media do
  @moduledoc """
  Image work on uploads (architecture 3 and 12), done in Rust on a dirty
  CPU scheduler (`native/lumia_media`): the format is told from the bytes,
  the declared dimensions are checked before anything is decoded, the
  decode runs under an allocation limit, and what comes back was encoded
  by the instance from the pixels, so none of the upload's own bytes or
  metadata is ever stored or served.

  The checks here are the cheap ones in front of that: the upload's size,
  and how many are being worked on at once (each may hold a few hundred
  megabytes while it is, so the two bounds together are what an instance
  on a small machine can afford beside everything else it does).
  """

  alias Lumia.Media.Native

  @max_upload_bytes 8 * 1024 * 1024
  @at_once 2

  @doc "The largest upload taken, in bytes."
  def max_upload_bytes, do: @max_upload_bytes

  @doc """
  An uploaded picture as an avatar: a square from its middle, at most 512
  pixels a side, as JPEG (or PNG when it has transparency).

  `{:ok, {media_type, bytes}}`, or `{:error, reason}`: `:too_large` (over
  #{div(@max_upload_bytes, 1024 * 1024)} MB, over 8,192 pixels a side or
  25 million in all), `:unsupported` (not PNG, JPEG, WebP or GIF),
  `:invalid` (one of those, and it would not decode), or `:rate_limited`
  when #{@at_once} are already being worked on.
  """
  def avatar(upload) when is_binary(upload) do
    cond do
      byte_size(upload) > @max_upload_bytes -> {:error, :too_large}
      upload == "" -> {:error, :unsupported}
      true -> bounded(fn -> Native.avatar(upload) end)
    end
  end

  defp bounded(work) do
    slots = slots()

    if :atomics.add_get(slots, 1, 1) > @at_once do
      :atomics.sub(slots, 1, 1)
      {:error, :rate_limited}
    else
      try do
        work.()
      rescue
        # A panic in the decoder, caught by Rustler and raised here.
        _ -> {:error, :invalid}
      after
        :atomics.sub(slots, 1, 1)
      end
    end
  end

  defp slots do
    case :persistent_term.get(__MODULE__, nil) do
      nil ->
        slots = :atomics.new(1, signed: true)
        :persistent_term.put(__MODULE__, slots)
        slots

      slots ->
        slots
    end
  end
end
