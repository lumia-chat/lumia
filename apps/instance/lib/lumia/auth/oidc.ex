defmodule Lumia.Auth.Oidc do
  @moduledoc """
  OIDC sign-in (architecture 6.3), the instance as a client of one
  provider the operator names. `start/1` makes the provider's
  authorization URL (the code flow with PKCE and a nonce, through
  `Assent`) and keeps the flow by its state; `callback/1` exchanges the
  code, verifies the ID token against the provider's keys, finds the
  account bound to the provider's subject or makes one, and answers with
  a one-time sign-in token the client completes as it would a magic link
  (`MagicLink.complete/3`), so the web page lands as
  `{return}?magic={token}`.

  The provider's subject is the binding; an email in the claims fills the
  account's address on creation but never matches an existing account by
  itself (architecture 6.3: matching an email is never sufficient).
  """

  import Ecto.Query

  alias Assent.Strategy.OIDC
  alias Lumia.Accounts
  alias Lumia.Accounts.Account
  alias Lumia.Auth.MagicLink
  alias Lumia.Repo

  @flow_seconds 600

  defmodule Identity do
    @moduledoc false
    use Lumia.Schema

    schema "oidc_identities" do
      field :issuer, :string
      field :subject, :string
      field :account_id, Ecto.UUID
      timestamps()
    end
  end

  defmodule Flow do
    @moduledoc false
    use Lumia.Schema

    schema "oidc_flows" do
      field :state, :string
      field :session_params, :binary
      field :return_to, :string
      field :expires_at, :utc_datetime_usec
      timestamps(updated_at: false)
    end
  end

  @doc "The provider's name for the sign-in button; nil when there is no provider."
  def name do
    case settings() do
      nil -> nil
      settings -> Keyword.get(settings, :name, "Sign in with your organisation")
    end
  end

  @doc "Whether the instance has a provider."
  def enabled?, do: settings() != nil

  @doc """
  Begins a sign-in: the provider's authorization URL, with the flow kept
  for ten minutes. `return_to` is where the browser lands afterwards: the
  instance's own UI or a trusted web origin, nothing else.
  """
  def start(return_to) do
    with {:ok, settings} <- fetch_settings(),
         :ok <- check_return(return_to),
         nonce = Base.url_encode64(:crypto.strong_rand_bytes(16), padding: false),
         {:ok, %{url: url, session_params: params}} <-
           OIDC.authorize_url(Keyword.put(config(settings), :nonce, nonce)) do
      {:ok, _} =
        Repo.insert(%Flow{
          state: params.state,
          session_params: :erlang.term_to_binary(params),
          return_to: return_to,
          expires_at: DateTime.add(DateTime.utc_now(), @flow_seconds, :second)
        })

      {:ok, url}
    else
      {:error, %{} = error} -> {:error, {:provider, Exception.message(error)}}
      other -> other
    end
  end

  @doc """
  Ends a sign-in with the provider's callback params: the flow's return
  URL with a one-time sign-in token, or with `error=oidc` when the code
  did not check out. The flow is spent either way.
  """
  def callback(%{"state" => state} = params) when is_binary(state) do
    with {:ok, settings} <- fetch_settings(),
         {:ok, flow} <- take_flow(state),
         {:ok, %{user: claims}} <- exchange(settings, flow, params),
         {:ok, account} <- account_for(settings, claims),
         {:ok, token} <- MagicLink.mint(account) do
      {:ok, with_query(flow.return_to, "magic=" <> token)}
    else
      {:error, {:flow, return_to}} -> {:error, with_query(return_to, "error=oidc")}
      _ -> {:error, nil}
    end
  end

  def callback(_params), do: {:error, nil}

  # ---- Internals -------------------------------------------------------------------

  defp settings, do: Application.get_env(:lumia, :oidc)

  defp fetch_settings do
    case settings() do
      nil -> {:error, :policy_denied}
      settings -> {:ok, settings}
    end
  end

  defp config(settings) do
    [
      client_id: Keyword.fetch!(settings, :client_id),
      client_secret: Keyword.get(settings, :client_secret, ""),
      base_url: Keyword.fetch!(settings, :issuer),
      redirect_uri: LumiaWeb.Endpoint.url() <> "/api/v1/auth/oidc/callback",
      authorization_params: [scope: "openid email profile"],
      code_verifier: true
    ]
  end

  # The native apps take the token back by their own URL scheme (the system
  # browser sheet returns to the app on it); the token is one-time and
  # short-lived, which is what bounds a stranger's app claiming the scheme.
  @app_scheme "lumia"

  defp check_return(return_to) when is_binary(return_to) do
    %URI{scheme: scheme, host: host, port: port} = URI.parse(return_to)
    origin = "#{scheme}://#{host}#{port_suffix(scheme, port)}"
    allowed = [LumiaWeb.Endpoint.url() | Lumia.Identity.web_origins()]

    cond do
      scheme == @app_scheme -> :ok
      scheme in ["http", "https"] and origin in allowed -> :ok
      true -> {:error, :invalid_argument}
    end
  end

  defp check_return(_), do: {:error, :invalid_argument}

  defp port_suffix("https", 443), do: ""
  defp port_suffix("http", 80), do: ""
  defp port_suffix(_, nil), do: ""
  defp port_suffix(_, port), do: ":#{port}"

  defp take_flow(state) do
    case Repo.one(from f in Flow, where: f.state == ^state) do
      nil ->
        {:error, :unauthenticated}

      flow ->
        Repo.delete(flow)

        if DateTime.after?(flow.expires_at, DateTime.utc_now()),
          do: {:ok, flow},
          else: {:error, {:flow, flow.return_to}}
    end
  end

  defp exchange(settings, flow, params) do
    session_params = :erlang.binary_to_term(flow.session_params, [:safe])

    case OIDC.callback(Keyword.put(config(settings), :session_params, session_params), params) do
      {:ok, result} -> {:ok, result}
      {:error, _} -> {:error, {:flow, flow.return_to}}
    end
  end

  defp account_for(settings, %{"sub" => subject} = claims) when is_binary(subject) do
    issuer = Keyword.fetch!(settings, :issuer)

    case Repo.get_by(Identity, issuer: issuer, subject: subject) do
      %Identity{account_id: id} ->
        {:ok, Repo.get!(Account, id)}

      nil ->
        Lumia.Tx.transact(fn ->
          with {:ok, account} <- create_account(claims),
               {:ok, _} <-
                 Repo.insert(%Identity{issuer: issuer, subject: subject, account_id: account.id}) do
            {:ok, account}
          end
        end)
    end
  end

  defp account_for(_, _), do: {:error, :unauthenticated}

  # A handle from the claims, made unique with a suffix before the insert
  # (a refused insert would abort the transaction); the email only when no
  # account has it.
  defp create_account(claims) do
    base =
      [claims["preferred_username"], claims["email"] |> to_string() |> String.split("@") |> hd()]
      |> Enum.find("user", &(is_binary(&1) and &1 != ""))
      |> String.downcase()
      |> String.replace(~r/[^a-z0-9_]/, "")
      |> String.slice(0, 24)
      |> then(&if(&1 == "", do: "user", else: &1))

    email =
      case claims["email"] do
        e when is_binary(e) and e != "" ->
          if Repo.exists?(
               from a in Account, where: fragment("lower(?)", a.email) == ^String.downcase(e)
             ),
             do: nil,
             else: e

        _ ->
          nil
      end

    handle =
      Enum.find_value(0..50, fn attempt ->
        candidate =
          if attempt == 0, do: base, else: base <> Integer.to_string(:rand.uniform(9999))

        if Repo.exists?(from a in Account, where: fragment("lower(?)", a.handle) == ^candidate),
          do: nil,
          else: candidate
      end)

    case handle do
      nil ->
        {:error, :already_exists}

      handle ->
        Accounts.create_account(%{handle: handle, email: email, display_name: claims["name"]})
    end
  end

  defp with_query(url, query) do
    if String.contains?(url, "?"), do: url <> "&" <> query, else: url <> "?" <> query
  end
end
