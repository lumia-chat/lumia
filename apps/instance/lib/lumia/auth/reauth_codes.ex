defmodule Lumia.Auth.ReauthCodes do
  @moduledoc """
  Reauth by a mailed code (specs/identity.md 10), for an account that
  signs in by magic link and has nothing else to prove itself with. Six
  digits mailed to the account's address, bound to the session that asked,
  good once within fifteen minutes; five wrong attempts spend it. Five an
  hour per account. Only an HMAC of the code under the instance's key is
  stored, so the table alone does not give the codes away.
  """

  import Ecto.Query
  import Swoosh.Email, except: [from: 2]

  alias Lumia.Accounts.Account
  alias Lumia.Auth.MagicLink
  alias Lumia.Auth.Session
  alias Lumia.Crypto
  alias Lumia.Mailer
  alias Lumia.Repo

  @ttl_seconds 15 * 60
  @per_hour 5
  @attempts 5

  defmodule Code do
    @moduledoc false
    use Lumia.Schema

    schema "reauth_codes" do
      field :session_id, Ecto.UUID
      field :account_id, Ecto.UUID
      field :code_hash, :binary, redact: true
      field :attempts, :integer, default: 0
      field :expires_at, :utc_datetime_usec
      timestamps(updated_at: false)
    end
  end

  @doc """
  Mails a code for `session` to its account's address. `:ok`, or
  `{:error, :policy_denied}` when the instance sends no mail,
  `{:error, {:failed_precondition, "no_email"}}` for an account without an
  address, and `{:error, :rate_limited}` past five an hour.
  """
  def request(%Session{} = session) do
    account = Repo.get!(Account, session.account_id)

    cond do
      not MagicLink.enabled?() -> {:error, :policy_denied}
      account.email in [nil, ""] -> {:error, {:failed_precondition, "no_email"}}
      recent(account.id) >= @per_hour -> {:error, :rate_limited}
      true -> send_code(session, account)
    end
  end

  @doc """
  Checks a code for `session`: `:ok` once for the right one, which spends
  it, else `{:error, :unauthenticated}`. A wrong one counts against every
  live code of the session, and a code tried five times wrongly is gone.
  """
  def verify(%Session{id: session_id}, code) when is_binary(code) do
    now = DateTime.utc_now()
    presented = code_hash(String.trim(code))

    live =
      Repo.all(from c in Code, where: c.session_id == ^session_id and c.expires_at > ^now)

    case Enum.find(live, &Crypto.secure_compare(&1.code_hash, presented)) do
      %Code{} = hit ->
        Repo.delete!(hit)
        :ok

      nil ->
        Enum.each(live, &spend_attempt/1)
        {:error, :unauthenticated}
    end
  end

  def verify(_session, _code), do: {:error, :unauthenticated}

  @doc "How long a code lives, in seconds."
  def ttl_seconds, do: @ttl_seconds

  defp spend_attempt(%Code{attempts: attempts} = code) when attempts + 1 >= @attempts,
    do: Repo.delete!(code)

  defp spend_attempt(%Code{} = code),
    do: code |> Ecto.Changeset.change(attempts: code.attempts + 1) |> Repo.update!()

  defp recent(account_id) do
    since = DateTime.add(DateTime.utc_now(), -3600, :second)

    Repo.one(
      from c in Code,
        where: c.account_id == ^account_id and c.inserted_at > ^since,
        select: count()
    )
  end

  defp send_code(session, account) do
    <<n::unsigned-64>> = :crypto.strong_rand_bytes(8)
    code = n |> rem(1_000_000) |> Integer.to_string() |> String.pad_leading(6, "0")

    {:ok, _} =
      Repo.insert(%Code{
        session_id: session.id,
        account_id: account.id,
        code_hash: code_hash(code),
        expires_at: DateTime.add(DateTime.utc_now(), @ttl_seconds, :second)
      })

    {:ok, _} = Mailer.deliver(mail(account, code))
    :ok
  end

  defp mail(account, code) do
    host = Lumia.Identity.host()

    new()
    |> to({account.display_name || account.handle, account.email})
    |> Swoosh.Email.from({"Lumia on " <> host, "no-reply@" <> host})
    |> subject("Your Lumia confirmation code: " <> code)
    |> text_body("""
    Enter this code in Lumia to confirm it is you, within fifteen minutes:

    #{code}

    Lumia asks for it before something that changes your account's keys or
    devices. If you did not ask for it, someone signed in as you may be
    trying to: change how you sign in, and remove sessions you do not know.
    """)
  end

  defp code_hash(code), do: Crypto.hmac(key(), code)

  defp key do
    secret = Application.get_env(:lumia, LumiaWeb.Endpoint)[:secret_key_base]
    Plug.Crypto.KeyGenerator.generate(secret, "lumia/reauth-code/v1")
  end
end
