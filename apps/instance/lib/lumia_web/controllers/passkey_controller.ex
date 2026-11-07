defmodule LumiaWeb.PasskeyController do
  @moduledoc "Passkeys: the WebAuthn ceremonies and the account's list (architecture 6.3)."

  use Phoenix.Controller, formats: []

  alias Lumia.Accounts.Account
  alias Lumia.Auth.Passkeys
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def register_start(conn, _params) do
    account = Repo.get!(Account, conn.assigns.current_account_id)
    {:ok, id, options} = Passkeys.register_start(account)
    ProtoConn.render(conn, %V1.PasskeyChallenge{challenge_id: id, options_json: options})
  end

  def register_finish(conn, _params) do
    account = Repo.get!(Account, conn.assigns.current_account_id)

    with {:ok, req} <- ProtoConn.decode(conn, V1.PasskeyRegisterFinishRequest),
         {:ok, _} <-
           Passkeys.register_finish(account, req.challenge_id, req.credential_json, req.label) do
      ProtoConn.no_content(conn)
    end
  end

  # Public: the request options, for a handle when given.
  def assert_start(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.PasskeyAssertStartRequest) do
      {:ok, id, options} = Passkeys.assert_start(req.handle_or_email)
      ProtoConn.render(conn, %V1.PasskeyChallenge{challenge_id: id, options_json: options})
    end
  end

  # Public: the assertion signs the device in.
  def assert_finish(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.PasskeyAssertFinishRequest),
         {:ok, _session, tokens} <-
           Passkeys.assert_finish(
             req.challenge_id,
             req.credential_json,
             Proto.from_uuid(req.device_id) || Ecto.UUID.generate(),
             client: client(conn)
           ) do
      ProtoConn.render(conn, LumiaWeb.AuthController.token_pair(tokens))
    end
  end

  # The passkey as the second factor of a password sign-in.
  def set_required(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.PasskeyRequirementRequest),
         {:ok, _} <-
           Lumia.Accounts.set_passkey_required(conn.assigns.current_account_id, req.required) do
      ProtoConn.no_content(conn)
    end
  end

  def index(conn, _params) do
    passkeys = Passkeys.passkeys(conn.assigns.current_account_id)
    account = Lumia.Repo.get!(Lumia.Accounts.Account, conn.assigns.current_account_id)

    ProtoConn.render(conn, %V1.ListPasskeysResponse{
      required_for_password: account.passkey_required_at != nil,
      passkeys:
        Enum.map(passkeys, fn p ->
          %V1.Passkey{
            passkey_id: p.id,
            label: p.label,
            created_at: Proto.timestamp(p.inserted_at),
            last_used_at: Proto.timestamp(p.last_used_at)
          }
        end)
    })
  end

  def delete(conn, %{"passkey_id" => passkey_id}) do
    with :ok <- Passkeys.delete(conn.assigns.current_account_id, passkey_id) do
      ProtoConn.no_content(conn)
    end
  end

  defp client(conn), do: conn |> Plug.Conn.get_req_header("user-agent") |> List.first()
end
