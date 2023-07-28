defmodule Lumia.ServerFixtures do
  @moduledoc """
  This module defines test helpers for creating
  entities via the `Lumia.Server` context.
  """

  @doc """
  Generate a channel.
  """
  def channel_fixture(attrs \\ %{}) do
    {:ok, channel} =
      attrs
      |> Enum.into(%{
        description: "some description",
        is_private: true,
        name: "some name"
      })
      |> Lumia.Server.create_channel()

    channel
  end

  @doc """
  Generate a message.
  """
  def message_fixture(attrs \\ %{}) do
    {:ok, message} =
      attrs
      |> Enum.into(%{
        text: "some text"
      })
      |> Lumia.Server.create_message()

    message
  end
end
