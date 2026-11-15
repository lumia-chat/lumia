defmodule Lumia.Identity.LinkSessions do
  @moduledoc """
  Linking by prompt (specs/identity.md 4.2.1). The instance relays four
  small things between the new device and one that holds the identity,
  each once, in order: the request with a commitment, the answer, the
  reveal, the link bundle's code. It checks the reveal against the
  commitment, but the guarantee is the devices': each checks the other
  and shows the same six digits only if nothing was swapped on the way.

  Every step appends `LinkSessionUpdated` to the account's log, so the
  account's devices read the session again. A session lasts ten minutes;
  an account has at most five open.
  """

  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Accounts.Account
  alias Lumia.Auth.Session
  alias Lumia.Identity.LinkSession
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Tx
  alias Lumia.V1

  @ttl_seconds 600
  @open_per_account 5
  # A link bundle's code is eight characters; this bounds what is kept.
  @code_max_bytes 64

  @doc """
  Opens a session from the new device's session: its serialized
  `LinkRequest` and 32-byte commitment. `{:error, {:invalid_argument,
  field}}` for a request that is not one or a commitment of another
  length; `{:error, :rate_limited}` past five open for the account.
  """
  def open(%Session{} = session, request, commitment) do
    with :ok <- check_request(request),
         true <- byte_size(commitment) == 32 || {:error, {:invalid_argument, "commitment"}} do
      Tx.transact(fn -> insert_within_limit(session, request, commitment) end)
    end
  end

  defp insert_within_limit(session, request, commitment) do
    now = DateTime.utc_now()

    # The account's row, locked, so two opens at once count one another.
    Repo.one!(
      from a in Account, where: a.id == ^session.account_id, select: a.id, lock: "FOR UPDATE"
    )

    open_count =
      Repo.one(
        from l in LinkSession,
          where:
            l.account_id == ^session.account_id and l.expires_at > ^now and
              is_nil(l.declined_at) and is_nil(l.code),
          select: count()
      )

    if open_count >= @open_per_account do
      {:error, :rate_limited}
    else
      %LinkSession{
        account_id: session.account_id,
        opened_by: session.id,
        request: request,
        commitment: commitment,
        expires_at: DateTime.add(now, @ttl_seconds, :second)
      }
      |> Repo.insert()
      |> updated()
    end
  end

  @doc "The account's sessions still open: not expired, declined or done."
  def list_open(account_id) do
    now = DateTime.utc_now()

    Repo.all(
      from l in LinkSession,
        where:
          l.account_id == ^account_id and l.expires_at > ^now and is_nil(l.declined_at) and
            is_nil(l.code),
        order_by: [asc: l.inserted_at]
    )
  end

  @doc "One of the account's sessions, unexpired, or `{:error, :not_found}`."
  def get(account_id, id) do
    now = DateTime.utc_now()

    with {:ok, id} <- Ecto.UUID.cast(id) |> or_not_found(),
         %LinkSession{} = link <-
           Repo.one(
             from l in LinkSession,
               where: l.id == ^id and l.account_id == ^account_id and l.expires_at > ^now
           ) || {:error, :not_found} do
      {:ok, link}
    end
  end

  @doc """
  A device that holds the account's identity answers with its 32 bytes,
  once. `{:error, {:failed_precondition, "answered"}}` after another did
  (or the session was declined), `{:error, :permission_denied}` from a
  session whose device is not in the identity's held roster.
  """
  def answer(%Session{} = session, id, answer) do
    with true <- byte_size(answer) == 32 || {:error, {:invalid_argument, "answer"}},
         :ok <- holds_identity(session) do
      step(session.account_id, id, &first_answer(&1, session, answer))
    end
  end

  defp first_answer(link, session, answer) do
    if is_nil(link.answer) and is_nil(link.declined_at),
      do: {:ok, %{answer: answer, answered_by: session.id}},
      else: {:error, {:failed_precondition, "answered"}}
  end

  @doc """
  The session that opened it reveals its secret, once, after an answer and
  before a decline; it must match the commitment.
  """
  def reveal(%Session{} = session, id, reveal) do
    step(session.account_id, id, fn link ->
      cond do
        link.opened_by != session.id ->
          {:error, :permission_denied}

        not is_nil(link.declined_at) ->
          {:error, {:failed_precondition, "declined"}}

        is_nil(link.answer) or not is_nil(link.reveal) ->
          {:error, {:failed_precondition, "order"}}

        not Lumia.Crypto.secure_compare(commit(reveal), link.commitment) ->
          {:error, {:invalid_argument, "reveal"}}

        true ->
          {:ok, %{reveal: reveal}}
      end
    end)
  end

  @doc """
  The answering session posts the link bundle's code, once, after the
  reveal. `{:error, {:failed_precondition, "declined"}}` once any device
  declined the session.
  """
  def code(%Session{} = session, id, code) do
    step(session.account_id, id, fn link ->
      code = String.trim(code)

      cond do
        link.answered_by != session.id -> {:error, :permission_denied}
        not is_nil(link.declined_at) -> {:error, {:failed_precondition, "declined"}}
        is_nil(link.reveal) or not is_nil(link.code) -> {:error, {:failed_precondition, "order"}}
        code == "" or byte_size(code) > @code_max_bytes -> {:error, {:invalid_argument, "code"}}
        true -> {:ok, %{code: code}}
      end
    end)
  end

  @doc "Any session of the account declines it: it takes nothing more."
  def decline(%Session{} = session, id) do
    step(session.account_id, id, fn link ->
      if is_nil(link.code),
        do: {:ok, %{declined_at: DateTime.utc_now()}},
        else: {:error, {:failed_precondition, "order"}}
    end)
  end

  @doc "The protocol's `LinkSession` for a row."
  def to_proto(%LinkSession{} = link) do
    %V1.LinkSession{
      session_id: Proto.uuid(link.id),
      request: link.request,
      commitment: link.commitment,
      answer: link.answer || "",
      reveal: link.reveal || "",
      code: link.code || "",
      declined: not is_nil(link.declined_at),
      expires_at: Proto.timestamp(link.expires_at)
    }
  end

  @doc "Deletes expired sessions. Returns the count."
  def purge_expired do
    now = DateTime.utc_now()
    {n, _} = Repo.delete_all(from l in LinkSession, where: l.expires_at < ^now)
    n
  end

  defp commit(reveal), do: Lumia.Crypto.blake3("lumia/link-commit/v1" <> reveal)

  # A step on the session, under its row lock, then the account told.
  defp step(account_id, id, change) do
    Tx.transact(fn ->
      with {:ok, link} <- get(account_id, id),
           link = Repo.one!(from l in LinkSession, where: l.id == ^link.id, lock: "FOR UPDATE"),
           {:ok, fields} <- change.(link) do
        link |> Ecto.Changeset.change(fields) |> Repo.update() |> updated()
      end
    end)
  end

  defp updated({:ok, %LinkSession{} = link}) do
    {:ok, _} =
      Accounts.append_event(
        link.account_id,
        {:link_session_updated, %V1.LinkSessionUpdated{session_id: Proto.uuid(link.id)}}
      )

    {:ok, link}
  end

  defp updated(other), do: other

  defp check_request(bytes) do
    case safe_decode(bytes) do
      %V1.LinkRequest{device_id: id, ephemeral_public_key: eph}
      when byte_size(id) == 16 and byte_size(eph) == 32 ->
        :ok

      _ ->
        {:error, {:invalid_argument, "request"}}
    end
  end

  defp safe_decode(bytes) do
    V1.LinkRequest.decode(bytes)
  rescue
    _ -> nil
  end

  # The answering device must be one of the identity's: its session's device
  # in the roster this instance holds for the account's identity.
  defp holds_identity(%Session{} = session) do
    with %Account{identity_id: identity_id} when is_binary(identity_id) <-
           Repo.get(Account, session.account_id),
         %{devices: devices} <- Lumia.Identity.held_roster(identity_id),
         true <- Map.has_key?(devices, Ecto.UUID.dump!(session.device_id)) do
      :ok
    else
      _ -> {:error, :permission_denied}
    end
  end

  defp or_not_found({:ok, v}), do: {:ok, v}
  defp or_not_found(:error), do: {:error, :not_found}
end
