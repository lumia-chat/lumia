defmodule Lumia.MediaTest do
  use ExUnit.Case, async: true

  alias Lumia.Media

  @fixtures Path.expand("../support/fixtures/media", __DIR__)
  defp fixture(name), do: File.read!(Path.join(@fixtures, name))

  # A PNG's size is in its header; a JPEG's in its frame segment.
  defp size(<<0x89, "PNG\r\n", 0x1A, "\n", _::32, "IHDR", w::32, h::32, _::binary>>), do: {w, h}
  defp size(<<0xFF, 0xD8, rest::binary>>), do: jpeg_size(rest)

  defp jpeg_size(<<0xFF, marker, _len::16, _precision, h::16, w::16, _::binary>>)
       when marker in [0xC0, 0xC1, 0xC2],
       do: {w, h}

  defp jpeg_size(<<0xFF, _marker, len::16, rest::binary>>) do
    skip = len - 2
    <<_::binary-size(^skip), rest::binary>> = rest
    jpeg_size(rest)
  end

  test "a photograph becomes a square JPEG without the camera's metadata" do
    upload = fixture("sideways.jpg")
    assert upload =~ "LumiaTestCam"
    assert {:ok, {"image/jpeg", avatar}} = Media.avatar(upload)
    assert size(avatar) == {320, 320}
    refute avatar =~ "LumiaTestCam"
    refute avatar =~ "Exif"
  end

  test "a large one is scaled to 512, a small one is left, transparency stays PNG" do
    assert {:ok, {"image/jpeg", big}} = Media.avatar(fixture("opaque.png"))
    assert size(big) == {512, 512}
    assert {:ok, {"image/jpeg", small}} = Media.avatar(fixture("small.png"))
    assert size(small) == {30, 30}
    assert {:ok, {"image/png", logo}} = Media.avatar(fixture("logo.png"))
    assert size(logo) == {200, 200}
    assert {:ok, {"image/jpeg", _}} = Media.avatar(fixture("two-frames.gif"))
    assert {:ok, {"image/jpeg", _}} = Media.avatar(fixture("wide.webp"))
  end

  test "what is not a picture, is broken, or is too large is refused" do
    assert {:error, :unsupported} = Media.avatar(fixture("not-a-picture.txt"))
    assert {:error, :unsupported} = Media.avatar("")
    assert {:error, :invalid} = Media.avatar(fixture("broken.png"))
    assert {:error, :too_large} = Media.avatar(fixture("claims-huge.png"))
    over = :binary.copy(<<0>>, Media.max_upload_bytes() + 1)
    assert {:error, :too_large} = Media.avatar(over)
  end
end
