defmodule LumiaWeb.ChatLive.Show do
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
    chat = Server.get_chat!(id)
    message_changeset = Server.change_message(%Message{chat_id: chat.id, user_id: socket.assigns.current_user.id})
    messages = Server.list_recent_messages(chat.id) |> Repo.preload(:user)

    {:noreply,
     socket
     |> assign(:page_title, page_title(socket.assigns.live_action))
     |> assign(:chat, chat)
     |> assign(:message_form, to_form(message_changeset))
     |> assign(:messages, messages)}
  end

  @impl true
  def handle_event("validate-message", %{}, socket) do
    {:noreply, socket}
  end

  def handle_event("add-message", %{"message" => message_params}, socket) do
    case Server.create_message(socket.assigns.chat.id, socket.assigns.current_user.id, message_params) do
      {:ok, _message} ->
        {:noreply, assign(socket, :message_form, message_changeset(socket) |> to_form())}

      {:error, changeset} ->
        {:noreply, assign(socket, :message_form, to_form(changeset))}
    end
  end

  @impl true
  def handle_info({:new_message, message}, socket) do
    {:noreply, update(socket, :messages, &(&1 ++ [message]))}
  end

  defp page_title(:show), do: "Show Chat"
  defp page_title(:edit), do: "Edit Chat"

  defp message_changeset(socket) do
    %Message{chat_id: socket.assigns.chat.id, user_id: socket.assigns.current_user.id}
    |> Server.change_message()
  end

  defp relative_time(datetime) do
    Timex.Format.DateTime.Formatters.Relative.format!(datetime, "{relative}")
  end

  defp iso_time(datetime) do
    Timex.format!(datetime, "{ISO:Basic:Z}")
  end
end
