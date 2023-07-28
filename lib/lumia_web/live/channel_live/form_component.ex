defmodule LumiaWeb.ChannelLive.FormComponent do
  use LumiaWeb, :live_component

  alias Lumia.Server

  @impl true
  def render(assigns) do
    ~H"""
    <div>
      <.header>
        <%= @title %>
        <:subtitle>Use this form to manage channel records in your database.</:subtitle>
      </.header>

      <.simple_form
        for={@form}
        id="channel-form"
        phx-target={@myself}
        phx-change="validate"
        phx-submit="save"
      >
        <.input field={@form[:name]} type="text" label="Name" />
        <.input field={@form[:description]} type="text" label="Description" />
        <.input field={@form[:is_private]} type="checkbox" label="Is private" />
        <:actions>
          <.button phx-disable-with="Saving...">Save Channel</.button>
        </:actions>
      </.simple_form>
    </div>
    """
  end

  @impl true
  def update(%{channel: channel} = assigns, socket) do
    changeset = Server.change_channel(channel)

    {:ok,
     socket
     |> assign(assigns)
     |> assign_form(changeset)}
  end

  @impl true
  def handle_event("validate", %{"channel" => channel_params}, socket) do
    changeset =
      socket.assigns.channel
      |> Server.change_channel(channel_params)
      |> Map.put(:action, :validate)

    {:noreply, assign_form(socket, changeset)}
  end

  def handle_event("save", %{"channel" => channel_params}, socket) do
    save_channel(socket, socket.assigns.action, channel_params)
  end

  defp save_channel(socket, :edit, channel_params) do
    case Server.update_channel(socket.assigns.channel, channel_params) do
      {:ok, channel} ->
        notify_parent({:saved, channel})

        {:noreply,
         socket
         |> put_flash(:info, "Channel updated successfully")
         |> push_patch(to: socket.assigns.patch)}

      {:error, %Ecto.Changeset{} = changeset} ->
        {:noreply, assign_form(socket, changeset)}
    end
  end

  defp save_channel(socket, :new, channel_params) do
    case Server.create_channel(channel_params) do
      {:ok, channel} ->
        notify_parent({:saved, channel})

        {:noreply,
         socket
         |> put_flash(:info, "Channel created successfully")
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
