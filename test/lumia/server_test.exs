defmodule Lumia.ServerTest do
  use Lumia.DataCase

  alias Lumia.Server

  describe "channels" do
    alias Lumia.Server.Channel

    import Lumia.ServerFixtures

    @invalid_attrs %{description: nil, is_private: nil, name: nil}

    test "list_channels/0 returns all channels" do
      channel = channel_fixture()
      assert Server.list_channels() == [channel]
    end

    test "get_channel!/1 returns the channel with given id" do
      channel = channel_fixture()
      assert Server.get_channel!(channel.id) == channel
    end

    test "create_channel/1 with valid data creates a channel" do
      valid_attrs = %{description: "some description", is_private: true, name: "some name"}

      assert {:ok, %Channel{} = channel} = Server.create_channel(valid_attrs)
      assert channel.description == "some description"
      assert channel.is_private == true
      assert channel.name == "some name"
    end

    test "create_channel/1 with invalid data returns error changeset" do
      assert {:error, %Ecto.Changeset{}} = Server.create_channel(@invalid_attrs)
    end

    test "update_channel/2 with valid data updates the channel" do
      channel = channel_fixture()

      update_attrs = %{
        description: "some updated description",
        is_private: false,
        name: "some updated name"
      }

      assert {:ok, %Channel{} = channel} = Server.update_channel(channel, update_attrs)
      assert channel.description == "some updated description"
      assert channel.is_private == false
      assert channel.name == "some updated name"
    end

    test "update_channel/2 with invalid data returns error changeset" do
      channel = channel_fixture()
      assert {:error, %Ecto.Changeset{}} = Server.update_channel(channel, @invalid_attrs)
      assert channel == Server.get_channel!(channel.id)
    end

    test "delete_channel/1 deletes the channel" do
      channel = channel_fixture()
      assert {:ok, %Channel{}} = Server.delete_channel(channel)
      assert_raise Ecto.NoResultsError, fn -> Server.get_channel!(channel.id) end
    end

    test "change_channel/1 returns a channel changeset" do
      channel = channel_fixture()
      assert %Ecto.Changeset{} = Server.change_channel(channel)
    end
  end

  describe "messages" do
    alias Lumia.Server.Message

    import Lumia.ServerFixtures

    @invalid_attrs %{text: nil}

    test "list_messages/0 returns all messages" do
      message = message_fixture()
      assert Server.list_messages() == [message]
    end

    test "get_message!/1 returns the message with given id" do
      message = message_fixture()
      assert Server.get_message!(message.id) == message
    end

    test "create_message/1 with valid data creates a message" do
      valid_attrs = %{text: "some text"}

      assert {:ok, %Message{} = message} = Server.create_message(valid_attrs)
      assert message.text == "some text"
    end

    test "create_message/1 with invalid data returns error changeset" do
      assert {:error, %Ecto.Changeset{}} = Server.create_message(@invalid_attrs)
    end

    test "update_message/2 with valid data updates the message" do
      message = message_fixture()
      update_attrs = %{text: "some updated text"}

      assert {:ok, %Message{} = message} = Server.update_message(message, update_attrs)
      assert message.text == "some updated text"
    end

    test "update_message/2 with invalid data returns error changeset" do
      message = message_fixture()
      assert {:error, %Ecto.Changeset{}} = Server.update_message(message, @invalid_attrs)
      assert message == Server.get_message!(message.id)
    end

    test "delete_message/1 deletes the message" do
      message = message_fixture()
      assert {:ok, %Message{}} = Server.delete_message(message)
      assert_raise Ecto.NoResultsError, fn -> Server.get_message!(message.id) end
    end

    test "change_message/1 returns a message changeset" do
      message = message_fixture()
      assert %Ecto.Changeset{} = Server.change_message(message)
    end
  end
end
