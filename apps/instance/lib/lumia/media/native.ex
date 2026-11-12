defmodule Lumia.Media.Native do
  @moduledoc false
  # native/lumia_media, built by Rustler when the instance compiles. Always
  # optimized: a debug build takes seconds over a phone's photograph.
  use Rustler, otp_app: :lumia, crate: "lumia_media", mode: :release

  def avatar(_upload), do: :erlang.nif_error(:nif_not_loaded)
end
