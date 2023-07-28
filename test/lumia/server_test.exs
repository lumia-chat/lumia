defmodule Lumia.ServerTest do
  use Lumia.DataCase

  alias Lumia.Server

  describe "chats" do
    alias Lumia.Server.Chat

    import Lumia.ServerFixtures

    @invalid_attrs %{description: nil, is_private: nil, name: nil}

    test "list_chats/0 returns all chats" do
      chat = chat_fixture()
      assert Server.list_chats() == [chat]
    end

    test "get_chat!/1 returns the chat with given id" do
      chat = chat_fixture()
      assert Server.get_chat!(chat.id) == chat
    end

    test "create_chat/1 with valid data creates a chat" do
      valid_attrs = %{description: "some description", is_private: true, name: "some name"}

      assert {:ok, %Chat{} = chat} = Server.create_chat(valid_attrs)
      assert chat.description == "some description"
      assert chat.is_private == true
      assert chat.name == "some name"
    end

    test "create_chat/1 with invalid data returns error changeset" do
      assert {:error, %Ecto.Changeset{}} = Server.create_chat(@invalid_attrs)
    end

    test "update_chat/2 with valid data updates the chat" do
      chat = chat_fixture()

      update_attrs = %{
        description: "some updated description",
        is_private: false,
        name: "some updated name"
      }

      assert {:ok, %Chat{} = chat} = Server.update_chat(chat, update_attrs)
      assert chat.description == "some updated description"
      assert chat.is_private == false
      assert chat.name == "some updated name"
    end

    test "update_chat/2 with invalid data returns error changeset" do
      chat = chat_fixture()
      assert {:error, %Ecto.Changeset{}} = Server.update_chat(chat, @invalid_attrs)
      assert chat == Server.get_chat!(chat.id)
    end

    test "delete_chat/1 deletes the chat" do
      chat = chat_fixture()
      assert {:ok, %Chat{}} = Server.delete_chat(chat)
      assert_raise Ecto.NoResultsError, fn -> Server.get_chat!(chat.id) end
    end

    test "change_chat/1 returns a chat changeset" do
      chat = chat_fixture()
      assert %Ecto.Changeset{} = Server.change_chat(chat)
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
