defmodule Lumia.Auth.Passkeys do
  @moduledoc """
  Passkeys (architecture 6.3): WebAuthn credentials an account registers
  while signed in and signs in with afterwards, through `Wax`. The
  ceremonies speak the browser's own JSON (the options this module makes,
  the credential `PublicKeyCredential.toJSON()` answers with), so every
  client hands them to its platform's WebAuthn call unchanged. A pending
  challenge lives in `webauthn_challenges` for five minutes, single use.

  The relying party is the instance's host, and the accepted origins are
  the instance's own URL and the web origins it trusts for web devices.
  Attestation is not required: a passkey proves possession of a key, not
  a make of authenticator.
  """

  import Ecto.Query

  alias Lumia.Accounts.Account
  alias Lumia.Auth
  alias Lumia.Repo

  @challenge_seconds 300

  defmodule Passkey do
    @moduledoc false
    use Lumia.Schema

    schema "passkeys" do
      field :account_id, Ecto.UUID
      field :credential_id, :binary
      field :public_key, :binary
      field :sign_count, :integer, default: 0
      field :label, :string, default: ""
      field :last_used_at, :utc_datetime_usec
      timestamps()
    end
  end

  defmodule Challenge do
    @moduledoc false
    use Lumia.Schema

    schema "webauthn_challenges" do
      field :kind, :string
      field :account_id, Ecto.UUID
      field :challenge, :binary
      field :expires_at, :utc_datetime_usec
      timestamps(updated_at: false)
    end
  end

  # ---- Registration ---------------------------------------------------------------

  @doc "The creation options for a signed-in account, and the challenge's id."
  def register_start(%Account{} = account) do
    challenge = Wax.new_registration_challenge(wax_opts())
    {:ok, id} = keep("register", account.id, challenge)

    exclude =
      for p <- passkeys(account.id), do: %{type: "public-key", id: b64(p.credential_id)}

    options = %{
      challenge: b64(challenge.bytes),
      rp: %{id: rp_id(), name: "Lumia on " <> rp_id()},
      user: %{
        id: b64(Ecto.UUID.dump!(account.id)),
        name: account.handle,
        displayName: blank_or(account.display_name, account.handle)
      },
      pubKeyCredParams: [
        %{type: "public-key", alg: -7},
        %{type: "public-key", alg: -257}
      ],
      authenticatorSelection: %{residentKey: "preferred", userVerification: "preferred"},
      attestation: "none",
      excludeCredentials: exclude,
      timeout: @challenge_seconds * 1000
    }

    {:ok, id, Jason.encode!(options)}
  end

  @doc "Stores the credential the browser made for the challenge."
  def register_finish(%Account{} = account, challenge_id, credential_json, label) do
    with {:ok, challenge} <- take(challenge_id, "register", account.id),
         {:ok, cred} <- decode_credential(credential_json),
         {:ok, attestation} <- b64_field(cred, ["response", "attestationObject"]),
         {:ok, client_data} <- b64_field(cred, ["response", "clientDataJSON"]),
         {:ok, {auth_data, _}} <- Wax.register(attestation, client_data, challenge) |> wrap(),
         %{attested_credential_data: %{credential_id: id, credential_public_key: key}} <-
           auth_data do
      %Passkey{
        account_id: account.id,
        credential_id: id,
        public_key: :erlang.term_to_binary(key),
        sign_count: auth_data.sign_count,
        label: String.slice(String.trim(label || ""), 0, 80)
      }
      |> Ecto.Changeset.change()
      |> Ecto.Changeset.unique_constraint(:credential_id)
      |> Repo.insert()
      |> case do
        {:ok, passkey} -> {:ok, passkey}
        {:error, _} -> {:error, :already_exists}
      end
    else
      {:error, reason} -> {:error, reason}
      _ -> {:error, :invalid_argument}
    end
  end

  # ---- Sign-in ---------------------------------------------------------------------

  @doc """
  The request options and the challenge's id. With a handle or email, the
  account's passkeys go into `allowCredentials` (an unknown one gets an
  empty list, which says nothing); without one, a discoverable passkey is
  expected.
  """
  def assert_start(handle_or_email) do
    allow =
      case account_for(handle_or_email) do
        nil ->
          []

        account ->
          for p <- passkeys(account.id), do: %{type: "public-key", id: b64(p.credential_id)}
      end

    challenge = Wax.new_authentication_challenge(wax_opts())
    {:ok, id} = keep("assert", nil, challenge)

    options = %{
      challenge: b64(challenge.bytes),
      rpId: rp_id(),
      allowCredentials: allow,
      userVerification: "preferred",
      timeout: @challenge_seconds * 1000
    }

    {:ok, id, Jason.encode!(options)}
  end

  @doc "Signs `device_id` in with the assertion the browser made for the challenge."
  def assert_finish(challenge_id, credential_json, device_id, opts \\ []) do
    with {:ok, challenge} <- take(challenge_id, "assert", nil),
         {:ok, cred} <- decode_credential(credential_json),
         {:ok, raw_id} <- b64_field(cred, ["rawId"]),
         {:ok, auth_data_bin} <- b64_field(cred, ["response", "authenticatorData"]),
         {:ok, signature} <- b64_field(cred, ["response", "signature"]),
         {:ok, client_data} <- b64_field(cred, ["response", "clientDataJSON"]),
         %Passkey{} = passkey <- Repo.get_by(Passkey, credential_id: raw_id),
         key = :erlang.binary_to_term(passkey.public_key, [:safe]),
         {:ok, auth_data} <-
           Wax.authenticate(raw_id, auth_data_bin, signature, client_data, challenge, [
             {raw_id, key}
           ])
           |> wrap(),
         :ok <- fresh?(passkey, auth_data.sign_count),
         {:ok, _} <-
           passkey
           |> Ecto.Changeset.change(
             sign_count: auth_data.sign_count,
             last_used_at: DateTime.utc_now()
           )
           |> Repo.update() do
      Auth.create_session(passkey.account_id, device_id, opts)
    else
      _ -> {:error, :unauthenticated}
    end
  end

  # ---- Management ------------------------------------------------------------------

  @doc "An account's passkeys, oldest first."
  def passkeys(account_id) do
    Repo.all(from p in Passkey, where: p.account_id == ^account_id, order_by: p.inserted_at)
  end

  @doc "Removes one of the account's passkeys."
  def delete(account_id, passkey_id) do
    case Repo.get_by(Passkey, id: passkey_id, account_id: account_id) do
      nil -> {:error, :not_found}
      passkey -> Repo.delete(passkey) |> then(fn {:ok, _} -> :ok end)
    end
  end

  # ---- Internals -------------------------------------------------------------------

  defp wax_opts do
    [
      origin: origins(),
      rp_id: rp_id(),
      attestation: "none",
      trusted_attestation_types: [:none, :basic, :self, :attca, :anonca, :uncertain],
      verify_trust_root: false,
      user_verification: "preferred",
      timeout: @challenge_seconds
    ]
  end

  defp rp_id, do: Lumia.Identity.host()

  defp origins do
    [LumiaWeb.Endpoint.url() | Lumia.Identity.web_origins()] |> Enum.uniq()
  end

  defp keep(kind, account_id, challenge) do
    %Challenge{
      kind: kind,
      account_id: account_id,
      challenge: :erlang.term_to_binary(challenge),
      expires_at: DateTime.add(DateTime.utc_now(), @challenge_seconds, :second)
    }
    |> Repo.insert()
    |> case do
      {:ok, row} -> {:ok, row.id}
      other -> other
    end
  end

  # Single use: the row goes whether the ceremony then succeeds or not.
  defp take(id, kind, account_id) do
    with {:ok, _} <- Ecto.UUID.cast(id),
         %Challenge{} = row <- Repo.get_by(Challenge, id: id, kind: kind),
         true <- row.account_id == account_id,
         {:ok, _} <- Repo.delete(row),
         true <- DateTime.after?(row.expires_at, DateTime.utc_now()) do
      {:ok, :erlang.binary_to_term(row.challenge, [:safe])}
    else
      _ -> {:error, :unauthenticated}
    end
  end

  defp fresh?(%Passkey{sign_count: before}, count) when count == 0 or count > before, do: :ok
  defp fresh?(_, _), do: {:error, :unauthenticated}

  defp account_for(handle_or_email) when is_binary(handle_or_email) do
    needle = handle_or_email |> String.trim() |> String.downcase()

    if needle == "",
      do: nil,
      else:
        Repo.one(
          from a in Account,
            where:
              fragment("lower(?)", a.handle) == ^needle or
                fragment("lower(?)", a.email) == ^needle
        )
  end

  defp account_for(_), do: nil

  defp decode_credential(json) when is_binary(json) do
    case Jason.decode(json) do
      {:ok, %{} = map} -> {:ok, map}
      _ -> {:error, :invalid_argument}
    end
  end

  defp decode_credential(_), do: {:error, :invalid_argument}

  defp b64_field(map, path) do
    case get_in(map, path) do
      value when is_binary(value) ->
        case Base.url_decode64(value, padding: false) do
          {:ok, bytes} -> {:ok, bytes}
          :error -> {:error, :invalid_argument}
        end

      _ ->
        {:error, :invalid_argument}
    end
  end

  defp wrap({:ok, value}), do: {:ok, value}
  defp wrap({:error, _}), do: {:error, :unauthenticated}

  defp b64(bytes), do: Base.url_encode64(bytes, padding: false)
  defp blank_or(nil, other), do: other
  defp blank_or("", other), do: other
  defp blank_or(value, _), do: value
end
