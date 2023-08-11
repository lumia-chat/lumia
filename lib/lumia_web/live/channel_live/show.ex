defmodule LumiaWeb.ChannelLive.Show do
  use LumiaWeb, :live_view

  alias Lumia.Repo
  alias Lumia.Server
  alias Lumia.Server.Message

  @impl true
  def mount(_params, _session, socket) do
    {:ok, socket}
  end

  @impl true
  def handle_params(%{"id" => id}, _, socket) do
    if connected?(socket), do: LumiaWeb.Endpoint.subscribe("messages:#{id}")
    channel = Server.get_channel!(id)

    message_changeset =
      Server.change_message(%Message{
        channel_id: channel.id,
        user_id: socket.assigns.current_user.id
      })

    messages =
      channel.id
      |> Server.list_recent_messages()
      |> Repo.preload(:user)

    last_message_time =
      case messages do
        [] -> nil
        [%{inserted_at: last_message_time} | _] -> last_message_time
      end

    {:noreply,
     socket
     |> assign(:page_title, page_title(socket.assigns.live_action))
     |> assign(:channel, channel)
     |> assign(:last_message_time, last_message_time)
     |> assign(:message_form, to_form(message_changeset))
     |> stream(:messages, messages)}
  end

  @impl true
  def handle_event("validate-message", %{}, socket) do
    {:noreply, socket}
  end

  def handle_event("load-previous", _, socket) do
    messages =
      socket.assigns.channel.id
      |> Server.list_recent_messages(before: socket.assigns.last_message_time)
      |> Repo.preload(:user)

    case messages do
      [] ->
        {:noreply, socket}

      [%{inserted_at: last_message_time} | _] = messages ->
        {:noreply,
         socket
         |> assign(:last_message_time, last_message_time)
         |> stream(:messages, Enum.reverse(messages), at: 0)}
    end
  end

  def handle_event("add-message", %{"message" => message_params}, socket) do
    case Server.create_message(
           socket.assigns.channel.id,
           socket.assigns.current_user.id,
           message_params
         ) do
      {:ok, _message} ->
        {:noreply, assign(socket, :message_form, message_changeset(socket) |> to_form())}

      {:error, changeset} ->
        {:noreply, assign(socket, :message_form, to_form(changeset))}
    end
  end

  @impl true
  def handle_info({:new_message, message}, socket) do
    {:noreply, stream_insert(socket, :messages, message)}
  end

  defp page_title(:show), do: "Show Channel"
  defp page_title(:edit), do: "Edit Channel"

  defp message_changeset(socket) do
    %Message{channel_id: socket.assigns.channel.id, user_id: socket.assigns.current_user.id}
    |> Server.change_message()
  end

  defp relative_time(datetime) do
    Timex.Format.DateTime.Formatters.Relative.format!(datetime, "{relative}")
  end

  defp iso_time(datetime) do
    Timex.format!(datetime, "{ISO:Basic:Z}")
  end

  def avatar_url(email) do
    hash =
      :crypto.hash(:md5, email)
      |> Base.encode16()
      |> String.downcase()

    "https://www.gravatar.com/avatar/#{hash}?s=200&d=mp"
  end
end
