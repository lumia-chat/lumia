defmodule Lumia.Server do
  @moduledoc """
  The Server context.
  """

  import Ecto.Query, warn: false

  alias Lumia.Accounts.User
  alias Lumia.Accounts.UserChannel

  alias Lumia.Server.Channel
  alias Lumia.Server.Message

  alias Lumia.Repo

  def broadcast({:error, error}), do: {:error, error}

  def broadcast({:ok, %Message{} = message}) do
    message = Repo.preload(message, :user)
    Phoenix.PubSub.broadcast(Lumia.PubSub, "messages:#{message.channel_id}", {:new_message, message})
    {:ok, message}
  end

  @doc """
  Returns the list of channels.

  ## Examples

      iex> list_channels()
      [%Channel{}, ...]

  """
  def list_channels do
    Repo.all(Channel)
  end

  def join_channel(%User{id: user_id}, %Channel{id: channel_id}) do
    now = DateTime.utc_now() |> DateTime.truncate(:second)

    Repo.insert(
      %UserChannel{user_id: user_id, channel_id: channel_id, inserted_at: now}
    )
  end

  @doc """
  Gets a single channel.

  Raises `Ecto.NoResultsError` if the Channel does not exist.

  ## Examples

      iex> get_channel!(123)
      %Channel{}

      iex> get_channel!(456)
      ** (Ecto.NoResultsError)

  """
  def get_channel!(id), do: Repo.get!(Channel, id)

  @doc """
  Creates a channel.

  ## Examples

      iex> create_channel(%{field: value})
      {:ok, %Channel{}}

      iex> create_channel(%{field: bad_value})
      {:error, %Ecto.Changeset{}}

  """
  def create_channel(attrs \\ %{}) do
    %Channel{}
    |> Channel.changeset(attrs)
    |> Repo.insert()
  end

  @doc """
  Updates a channel.

  ## Examples

      iex> update_channel(channel, %{field: new_value})
      {:ok, %Channel{}}

      iex> update_channel(channel, %{field: bad_value})
      {:error, %Ecto.Changeset{}}

  """
  def update_channel(%Channel{} = channel, attrs) do
    channel
    |> Channel.changeset(attrs)
    |> Repo.update()
  end

  @doc """
  Deletes a channel.

  ## Examples

      iex> delete_channel(channel)
      {:ok, %Channel{}}

      iex> delete_channel(channel)
      {:error, %Ecto.Changeset{}}

  """
  def delete_channel(%Channel{} = channel) do
    Repo.delete(channel)
  end

  @doc """
  Returns an `%Ecto.Changeset{}` for tracking channel changes.

  ## Examples

      iex> change_channel(channel)
      %Ecto.Changeset{data: %Channel{}}

  """
  def change_channel(%Channel{} = channel, attrs \\ %{}) do
    Channel.changeset(channel, attrs)
  end

  @doc """
  Returns the list of recent messages for a channel.

  ## Examples

      iex> list_recent_messages(channel_id)
      [%Message{}, ...]

  """
  def list_recent_messages(channel_id, opts \\ []) when is_binary(channel_id) do
    limit = Keyword.get(opts, :limit, 10)

    before =
      case Keyword.get(opts, :before) do
        nil -> true
        before -> dynamic([m], m.inserted_at < ^before)
      end

    Repo.all(
      from m in Message,
        where: m.channel_id == ^channel_id,
        where: ^before,
        order_by: [desc: m.inserted_at],
        limit: ^limit
    )
    |> Enum.reverse()
  end

  @doc """
  Gets a single message.

  Raises `Ecto.NoResultsError` if the Message does not exist.

  ## Examples

      iex> get_message!(123)
      %Message{}

      iex> get_message!(456)
      ** (Ecto.NoResultsError)

  """
  def get_message!(id), do: Repo.get!(Message, id)

  @doc """
  Creates a message.

  ## Examples

      iex> create_message(%{field: value})
      {:ok, %Message{}}

      iex> create_message(%{field: bad_value})
      {:error, %Ecto.Changeset{}}

  """
  def create_message(channel_id, user_id, attrs) when is_binary(channel_id) and is_binary(user_id) do
    %Message{channel_id: channel_id, user_id: user_id}
    |> Message.changeset(attrs)
    |> Repo.insert()
    |> broadcast()
  end

  @doc """
  Updates a message.

  ## Examples

      iex> update_message(message, %{field: new_value})
      {:ok, %Message{}}

      iex> update_message(message, %{field: bad_value})
      {:error, %Ecto.Changeset{}}

  """
  def update_message(%Message{} = message, attrs) do
    message
    |> Message.changeset(attrs)
    |> Repo.update()
  end

  @doc """
  Deletes a message.

  ## Examples

      iex> delete_message(message)
      {:ok, %Message{}}

      iex> delete_message(message)
      {:error, %Ecto.Changeset{}}

  """
  def delete_message(%Message{} = message) do
    Repo.delete(message)
  end

  @doc """
  Returns an `%Ecto.Changeset{}` for tracking message changes.

  ## Examples

      iex> change_message(message)
      %Ecto.Changeset{data: %Message{}}

  """
  def change_message(%Message{} = message, attrs \\ %{}) do
    Message.changeset(message, attrs)
  end
end
