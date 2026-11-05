defmodule Lumia.Auth.MagicLink do
  @moduledoc """
  Magic-link sign-in (architecture 6.3): a mail to an account's address
  carrying a token that signs the device in once, within fifteen minutes.
  Only the token's hash is stored. Asking for a link answers the same
  whether or not the address has an account, so the API gives away no
  addresses; the mail is sent only when it does. Per address, five links
  an hour (counted from the rows, as the invites are).
  """

  import Ecto.Query
  import Swoosh.Email, except: [from: 2]

  alias Lumia.Accounts.Account
  alias Lumia.Auth
  alias Lumia.Mailer
  alias Lumia.Repo

  @ttl_seconds 15 * 60
  @per_hour 5

  defmodule Link do
    @moduledoc false
    use Lumia.Schema

    schema "magic_links" do
      field :account_id, Ecto.UUID
      field :token_hash, :binary
      field :expires_at, :utc_datetime_usec
      field :used_at, :utc_datetime_usec
      timestamps(updated_at: false)
    end
  end

  @doc "Whether this instance sends magic links (it needs a mailer)."
  def enabled? do
    Application.get_env(:lumia, :instance, []) |> Keyword.get(:magic_link, false)
  end

  @doc """
  Sends a link to `email` when an account has it. `:ok` either way, or
  `{:error, :rate_limited}` past five an hour for the address, and
  `{:error, :policy_denied}` when the instance sends no mail.
  """
  def request(email) when is_binary(email) do
    needle = email |> String.trim() |> String.downcase()

    cond do
      not enabled?() ->
        {:error, :policy_denied}

      needle == "" ->
        :ok

      true ->
        case Repo.one(from a in Account, where: fragment("lower(?)", a.email) == ^needle) do
          nil -> :ok
          account -> send_link(account)
        end
    end
  end

  @doc """
  A one-time sign-in token for the account, for a flow that proved the
  person another way (OIDC); nothing is mailed. Completed as a mailed
  token is.
  """
  def mint(%Account{} = account) do
    token = Base.url_encode64(:crypto.strong_rand_bytes(24), padding: false)
    expires_at = DateTime.add(DateTime.utc_now(), @ttl_seconds, :second)

    case Repo.insert(%Link{
           account_id: account.id,
           token_hash: hash(token),
           expires_at: expires_at
         }) do
      {:ok, _} -> {:ok, token}
      {:error, _} -> {:error, :internal}
    end
  end

  @doc "Signs `device_id` in with a token from the mail: once, within its lifetime."
  def complete(token, device_id, opts \\ []) when is_binary(token) do
    hash = hash(String.trim(token))
    now = DateTime.utc_now()

    Lumia.Tx.transact(fn ->
      with %Link{} = link <-
             Repo.one(from l in Link, where: l.token_hash == ^hash, lock: "FOR UPDATE"),
           true <- is_nil(link.used_at) and DateTime.after?(link.expires_at, now),
           {:ok, _} <- link |> Ecto.Changeset.change(used_at: now) |> Repo.update(),
           {:ok, session, tokens} <- Auth.create_session(link.account_id, device_id, opts) do
        {:ok, {session, tokens}}
      else
        _ -> {:error, :unauthenticated}
      end
    end)
    |> case do
      {:ok, {session, tokens}} -> {:ok, session, tokens}
      {:error, _} -> {:error, :unauthenticated}
    end
  end

  defp send_link(%Account{} = account) do
    since = DateTime.add(DateTime.utc_now(), -3600, :second)

    recent =
      Repo.one(
        from l in Link,
          where: l.account_id == ^account.id and l.inserted_at > ^since,
          select: count()
      )

    if recent >= @per_hour do
      {:error, :rate_limited}
    else
      token = Base.url_encode64(:crypto.strong_rand_bytes(24), padding: false)
      expires_at = DateTime.add(DateTime.utc_now(), @ttl_seconds, :second)

      {:ok, _} =
        Repo.insert(%Link{
          account_id: account.id,
          token_hash: hash(token),
          expires_at: expires_at
        })

      {:ok, _} = Mailer.deliver(mail(account, token))
      :ok
    end
  end

  defp mail(account, token) do
    host = Lumia.Identity.host()
    url = LumiaWeb.Endpoint.url() <> "/sign-in?magic=" <> token

    new()
    |> to({account.display_name || account.handle, account.email})
    |> Swoosh.Email.from({"Lumia on " <> host, "no-reply@" <> host})
    |> subject("Sign in to Lumia on " <> host)
    |> text_body("""
    Open this link on the device you are signing in on, within fifteen minutes:

    #{url}

    On a phone, paste this code into the app instead:

    #{token}

    If you did not ask for this, ignore it; nothing happens without the link.
    """)
  end

  defp hash(token), do: :crypto.hash(:sha256, token)
end
