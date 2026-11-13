defmodule Lumia.Invites do
  @moduledoc """
  Invite links (architecture 6.1, "the join flow", and 6.6): a member with
  `create_invites` mints a code for a server; anyone with the code sees
  the server's preview (public, so the trust card can show it before the
  joiner sends anything); accepting it adds the caller as a member, or as
  a guest when the invite says so. An invite carries an optional expiry
  and an optional use count (0 is unlimited). A code is one of the ways
  into an instance whose registration is closed: account creation with a
  valid code is allowed and joins the server in the same request.
  """

  import Ecto.Query

  alias Lumia.Audit
  alias Lumia.Events.Channel
  alias Lumia.Invites.Invite
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Servers.Member
  alias Lumia.Servers.Server
  alias Lumia.Tx

  @code_alphabet ~c"abcdefghijkmnpqrstuvwxyz23456789"
  @code_length 10
  @max_guest_days 365

  # Rate limits, Postgres-backed as architecture 7 asks for abuse-prone
  # operations: what one account minted or accepted in the last hour,
  # counted from the rows themselves (invites, and the audit log for
  # accepts), so N nodes share one count. `:invite_limits` in the
  # application environment overrides them (tests).
  @limits %{create_per_hour: 30, accept_per_hour: 20}

  @doc """
  Mints an invite for a server. The actor needs `create_invites` there.
  `attrs`: `channel_id` (a channel of the server, for a connected-channel
  invite), `expires_at`, `max_uses` (0 for unlimited), `as_guest` and
  `guest_days` (the guest membership's length; nil for no expiry).
  """
  def create(server_id, actor_id, attrs \\ %{}) do
    Tx.transact(fn ->
      with {:ok, actor} <- Servers.actor(server_id, actor_id),
           :ok <- permitted(actor, :create_invites),
           :ok <- within(:create_per_hour, minted_last_hour(actor_id)),
           {:ok, channel_id} <- check_channel(server_id, Map.get(attrs, :channel_id)),
           {:ok, guest_days} <- check_guest_days(Map.get(attrs, :guest_days)),
           {:ok, invite} <-
             Repo.insert(%Invite{
               code: code(),
               server_id: server_id,
               channel_id: channel_id,
               created_by: actor_id,
               expires_at: Map.get(attrs, :expires_at),
               max_uses: Map.get(attrs, :max_uses, 0) || 0,
               as_guest: Map.get(attrs, :as_guest, false) || false,
               guest_days: guest_days,
               email: Map.get(attrs, :email)
             }),
           {:ok, _} <-
             Audit.record({:server, server_id}, actor_id, "invite.create",
               after: %{"code" => invite.code, "max_uses" => invite.max_uses}
             ) do
        {:ok, invite}
      end
    end)
  end

  @doc """
  Emails an invite (architecture 6.2): one use, good for a week, to
  `email`, as a link to this instance's web page. The same permission,
  limit and `as_guest`/`guest_days` as `create/3`; the address stays on
  the invite, for the list. `{:error, {:failed_precondition, "mail"}}`
  when the instance sends no mail.
  """
  def email(server_id, actor_id, email, attrs \\ %{}) do
    email = String.trim(email || "")

    cond do
      not Lumia.Auth.MagicLink.enabled?() ->
        {:error, {:failed_precondition, "mail"}}

      not Regex.match?(~r/^[^@\s]+@[^@\s]+\.[^@\s]+$/, email) or String.length(email) > 254 ->
        {:error, :invalid_argument}

      true ->
        attrs =
          attrs
          |> Map.put(:max_uses, 1)
          |> Map.put(:expires_at, DateTime.add(DateTime.utc_now(), 7, :day))
          |> Map.put(:email, email)

        with {:ok, invite} <- create(server_id, actor_id, attrs) do
          {:ok, _} = Lumia.Mailer.deliver(invite_mail(invite))
          {:ok, invite}
        end
    end
  end

  defp invite_mail(invite) do
    import Swoosh.Email, except: [from: 2]
    host = Lumia.Identity.host()
    server = Repo.get!(Lumia.Servers.Server, invite.server_id)
    url = LumiaWeb.Endpoint.url() <> "/invite/" <> invite.code

    new()
    |> to(invite.email)
    |> Swoosh.Email.from({"Lumia on " <> host, "no-reply@" <> host})
    |> subject("You are invited to " <> server.name <> " on " <> host)
    |> text_body("""
    Someone on #{host} invited you to #{server.name}. Open this link to join:

    #{url}

    It works once, within a week. If you were not expecting it, ignore it; nothing happens without the link.
    """)
  end

  @doc """
  A valid invite by code with what its preview shows: the server, the
  channel when it names one, and the member count. `{:error, :not_found}`
  for an unknown, expired or used-up code, which the preview shows alike
  (a code says nothing about what it once named).
  """
  def preview(code) do
    with {:ok, invite} <- valid(code) do
      server = Repo.get!(Server, invite.server_id)
      channel = invite.channel_id && Repo.get(Channel, invite.channel_id)

      count =
        Repo.one(from m in Member, where: m.server_id == ^invite.server_id, select: count())

      {:ok, %{invite: invite, server: server, channel: channel, member_count: count}}
    end
  end

  @doc "The invite behind a code when it can still be accepted."
  def valid(code) when is_binary(code) do
    case Repo.get_by(Invite, code: String.trim(code)) do
      nil -> {:error, :not_found}
      invite -> if usable?(invite), do: {:ok, invite}, else: {:error, :not_found}
    end
  end

  def valid(_), do: {:error, :not_found}

  @doc """
  Joins the account to the invite's server as a member, or as a guest
  with the invite's expiry. A use is counted per new membership; an
  account already in the server gets its membership back and the count
  stays. The row is locked so a last use is never given twice.
  """
  def accept(code, account_id) do
    Tx.transact(fn ->
      with :ok <- within(:accept_per_hour, accepted_last_hour(account_id)),
           {:ok, invite} <- lock(code) do
        join(invite, account_id)
      end
    end)
  end

  @doc "Revokes an invite: its creator, or a member with `manage_server`."
  def delete(code, actor_id) do
    Tx.transact(fn ->
      with {:ok, invite} <- fetch(code),
           :ok <- deletable(invite, actor_id),
           {:ok, _} <- Repo.delete(invite),
           {:ok, _} <-
             Audit.record({:server, invite.server_id}, actor_id, "invite.delete",
               after: %{"code" => invite.code}
             ) do
        {:ok, :deleted}
      end
    end)
    |> case do
      {:ok, :deleted} -> :ok
      other -> other
    end
  end

  @doc "The invites of a server, newest first. The actor needs `create_invites`."
  def list(server_id, actor_id) do
    with {:ok, actor} <- Servers.actor(server_id, actor_id),
         :ok <- permitted(actor, :create_invites) do
      {:ok,
       Repo.all(
         from i in Invite, where: i.server_id == ^server_id, order_by: [desc: i.inserted_at]
       )}
    end
  end

  # ---- Internals ---------------------------------------------------------------

  defp within(key, count) do
    limit = Application.get_env(:lumia, :invite_limits, %{}) |> Map.get(key, @limits[key])
    if count < limit, do: :ok, else: {:error, :rate_limited}
  end

  defp minted_last_hour(account_id) do
    since = DateTime.add(DateTime.utc_now(), -3600, :second)

    Repo.one(
      from i in Invite,
        where: i.created_by == ^account_id and i.inserted_at > ^since,
        select: count()
    )
  end

  defp accepted_last_hour(account_id) do
    since = DateTime.add(DateTime.utc_now(), -3600, :second)

    Repo.one(
      from e in Lumia.Audit.Entry,
        where:
          e.actor_account_id == ^account_id and e.action == "invite.accept" and
            e.created_at > ^since,
        select: count()
    )
  end

  defp join(invite, account_id) do
    case Repo.get_by(Member, server_id: invite.server_id, account_id: account_id) do
      %Member{} = member ->
        {:ok, member}

      nil ->
        opts =
          if invite.as_guest,
            do: [kind: "guest", expires_at: guest_expiry(invite.guest_days)],
            else: []

        opts = [{:channel_id, invite.channel_id} | opts]

        with {:ok, member} <-
               Servers.add_member(invite.server_id, account_id, invite.created_by, opts),
             {:ok, _} <- count_use(invite),
             {:ok, _} <-
               Audit.record({:server, invite.server_id}, account_id, "invite.accept",
                 after: %{"code" => invite.code, "kind" => member.kind}
               ) do
          {:ok, member}
        end
    end
  end

  defp count_use(invite) do
    invite
    |> Ecto.Changeset.change(uses: invite.uses + 1)
    |> Repo.update()
  end

  defp lock(code) when is_binary(code) do
    query = from i in Invite, where: i.code == ^String.trim(code), lock: "FOR UPDATE"

    case Repo.one(query) do
      nil -> {:error, :not_found}
      invite -> if usable?(invite), do: {:ok, invite}, else: {:error, :not_found}
    end
  end

  defp lock(_), do: {:error, :not_found}

  defp fetch(code) when is_binary(code) do
    case Repo.get_by(Invite, code: String.trim(code)) do
      nil -> {:error, :not_found}
      invite -> {:ok, invite}
    end
  end

  defp fetch(_), do: {:error, :not_found}

  defp usable?(invite) do
    unexpired =
      is_nil(invite.expires_at) or DateTime.after?(invite.expires_at, DateTime.utc_now())

    unspent = invite.max_uses == 0 or invite.uses < invite.max_uses
    unexpired and unspent
  end

  defp deletable(invite, actor_id) do
    if invite.created_by == actor_id do
      :ok
    else
      with {:ok, actor} <- Servers.actor(invite.server_id, actor_id) do
        permitted(actor, :manage_server)
      end
    end
  end

  defp permitted(actor, permission) do
    if actor.owner? or Lumia.Authz.Permission.has?(actor.server_permissions, permission),
      do: :ok,
      else: {:error, :permission_denied}
  end

  defp check_channel(_server_id, nil), do: {:ok, nil}
  defp check_channel(_server_id, ""), do: {:ok, nil}

  defp check_channel(server_id, channel_id) do
    case Repo.get(Channel, channel_id) do
      %Channel{server_id: ^server_id} -> {:ok, channel_id}
      _ -> {:error, :not_found}
    end
  end

  defp check_guest_days(nil), do: {:ok, nil}
  defp check_guest_days(0), do: {:ok, nil}

  defp check_guest_days(days) when is_integer(days) and days > 0 and days <= @max_guest_days,
    do: {:ok, days}

  defp check_guest_days(_), do: {:error, :invalid_argument}

  defp guest_expiry(nil), do: nil
  defp guest_expiry(days), do: DateTime.add(DateTime.utc_now(), days, :day)

  # Random from the alphabet without a modulo bias: the alphabet has 32
  # entries, so five bits pick one.
  defp code do
    @code_length
    |> :crypto.strong_rand_bytes()
    |> :binary.bin_to_list()
    |> Enum.map(&Enum.at(@code_alphabet, Bitwise.band(&1, 31)))
    |> List.to_string()
  end
end
