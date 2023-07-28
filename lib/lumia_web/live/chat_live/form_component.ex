defmodule LumiaWeb.ChatLive.FormComponent do
  use LumiaWeb, :live_component

  alias Lumia.Server

  @impl true
  def render(assigns) do
    ~H"""
    <div>
      <.header>
        <%= @title %>
        <:subtitle>Use this form to manage chat records in your database.</:subtitle>
      </.header>

      <.simple_form
        for={@form}
        id="chat-form"
        phx-target={@myself}
        phx-change="validate"
        phx-submit="save"
      >
        <.input field={@form[:name]} type="text" label="Name" />
        <.input field={@form[:description]} type="text" label="Description" />
        <.input field={@form[:is_private]} type="checkbox" label="Is private" />
        <:actions>
          <.button phx-disable-with="Saving...">Save Chat</.button>
        </:actions>
      </.simple_form>
    </div>
    """
  end

  @impl true
  def update(%{chat: chat} = assigns, socket) do
    changeset = Server.change_chat(chat)

    {:ok,
     socket
     |> assign(assigns)
     |> assign_form(changeset)}
  end

  @impl true
  def handle_event("validate", %{"chat" => chat_params}, socket) do
    changeset =
      socket.assigns.chat
      |> Server.change_chat(chat_params)
      |> Map.put(:action, :validate)

    {:noreply, assign_form(socket, changeset)}
  end

  def handle_event("save", %{"chat" => chat_params}, socket) do
    save_chat(socket, socket.assigns.action, chat_params)
  end

  defp save_chat(socket, :edit, chat_params) do
    case Server.update_chat(socket.assigns.chat, chat_params) do
      {:ok, chat} ->
        notify_parent({:saved, chat})

        {:noreply,
         socket
         |> put_flash(:info, "Chat updated successfully")
         |> push_patch(to: socket.assigns.patch)}

      {:error, %Ecto.Changeset{} = changeset} ->
        {:noreply, assign_form(socket, changeset)}
    end
  end

  defp save_chat(socket, :new, chat_params) do
    case Server.create_chat(chat_params) do
      {:ok, chat} ->
        notify_parent({:saved, chat})

        {:noreply,
         socket
         |> put_flash(:info, "Chat created successfully")
         |> push_patch(to: socket.assigns.patch)}

      {:error, %Ecto.Changeset{} = changeset} ->
        {:noreply, assign_form(socket, changeset)}
    end
  end

  defp assign_form(socket, %Ecto.Changeset{} = changeset) do
    assign(socket, :form, to_form(changeset))
  end

  defp notify_parent(msg), do: send(self(), {__MODULE__, msg})
end
