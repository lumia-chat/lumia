defmodule Lumia.Fixtures do
  @moduledoc "Builders for tests: accounts, channels, actors, and wire commands."

  alias Lumia.Accounts
  alias Lumia.Events
  alias Lumia.Proto
  alias Lumia.V1

  def account(name \\ "user") do
    {:ok, a} = Accounts.create_account(%{handle: "#{name}#{System.unique_integer([:positive])}"})
    a
  end

  def channel do
    {:ok, c} = Events.create_channel()
    c
  end

  def actor(account \\ nil) do
    account = account || account()
    %{account_id: account.id, device_id: Ecto.UUID.generate()}
  end

  @doc "A SendMessage command with a fresh client-minted id."
  def send_message(channel_id, body \\ "hello", opts \\ []) do
    %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel_id),
      op:
        {:send_message,
         %V1.SendMessage{body: body, parent_id: Proto.uuid(Keyword.get(opts, :parent_id))}}
    }
  end

  def edit_message(channel_id, message_id, body) do
    %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel_id),
      op: {:edit_message, %V1.EditMessage{message_id: Proto.uuid(message_id), body: body}}
    }
  end

  def delete_message(channel_id, message_id) do
    %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel_id),
      op: {:delete_message, %V1.DeleteMessage{message_id: Proto.uuid(message_id)}}
    }
  end

  def add_reaction(channel_id, message_id, reaction \\ "thumbs_up") do
    %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(channel_id),
      op: {:add_reaction, %V1.AddReaction{message_id: Proto.uuid(message_id), reaction: reaction}}
    }
  end
end
