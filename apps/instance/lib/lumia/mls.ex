defmodule Lumia.Mls do
  @moduledoc """
  The MLS delivery service (docs/specs/e2ee.md section 11), in Elixir with
  no MLS library: key-package pools per device with one last-resort
  package, and the newest GroupInfo per conversation. Every blob is opaque
  bytes; the only MLS fact this module reads is the integer epoch the
  publisher states. The epoch rule for `CONTROL` events lives in
  `Lumia.Events`, in the write transaction.
  """

  import Ecto.Query

  alias Lumia.Dms
  alias Lumia.Mls.GroupInfo
  alias Lumia.Mls.KeyPackage
  alias Lumia.Repo
  alias Lumia.Tx

  @lifetime_days 90

  # ---- Key packages --------------------------------------------------------------

  @doc """
  Adds `packages` to the device's pool with the 90-day lifetime. A
  non-empty `last_resort` replaces the device's previous last-resort
  package. Returns `{:ok, unclaimed_count}`.
  """
  def publish_key_packages(account_id, device_id, packages, last_resort)
      when is_list(packages) and is_binary(last_resort) do
    now = DateTime.utc_now()
    expires_at = DateTime.add(now, @lifetime_days, :day)

    rows =
      for bytes <- packages, bytes != "" do
        %{
          id: Lumia.Schema.generate_id(),
          account_id: account_id,
          device_id: device_id,
          key_package: bytes,
          last_resort: false,
          expires_at: expires_at,
          inserted_at: now
        }
      end

    Tx.transact(fn ->
      if rows != [], do: Repo.insert_all(KeyPackage, rows)
      if last_resort != "", do: replace_last_resort(account_id, device_id, last_resort, now)
      {:ok, key_package_count(account_id, device_id)}
    end)
  end

  defp replace_last_resort(account_id, device_id, bytes, now) do
    Repo.delete_all(
      from k in KeyPackage,
        where: k.account_id == ^account_id and k.device_id == ^device_id and k.last_resort
    )

    Repo.insert!(%KeyPackage{
      account_id: account_id,
      device_id: device_id,
      key_package: bytes,
      last_resort: true,
      expires_at: DateTime.add(now, @lifetime_days, :day),
      inserted_at: now
    })
  end

  @doc "Unclaimed, unexpired, non-last-resort packages the device holds here."
  def key_package_count(account_id, device_id) do
    now = DateTime.utc_now()

    Repo.aggregate(
      from(k in KeyPackage,
        where:
          k.account_id == ^account_id and k.device_id == ^device_id and not k.last_resort and
            k.expires_at > ^now
      ),
      :count
    )
  end

  @doc """
  Claims one key package per device of `target_account_id` (or of the one
  `device_id`), deleting each claimed row. An empty pool serves the device's
  last-resort package, which stays. The caller must share a `PERSONAL_*`
  channel with the target, or be the target (a device adding its
  identity's other devices), else `{:error, :not_found}`. The option
  `exclude_device:` leaves one device out, which is how a device skips
  itself. Returns
  `{:ok, [%{device_id: id, key_package: bytes}]}`; a device with nothing to
  serve is simply absent.
  """
  def claim(caller_account_id, target_account_id, device_id \\ nil, opts \\ []) do
    if caller_account_id == target_account_id or
         Dms.shares_personal_channel?(caller_account_id, target_account_id),
       do: Tx.transact(fn -> claim_devices(target_account_id, device_id, opts) end),
       else: {:error, :not_found}
  end

  defp claim_devices(account_id, device_id, opts) do
    now = DateTime.utc_now()
    exclude = Keyword.get(opts, :exclude_device)

    devices =
      if device_id,
        do: [device_id],
        else: account_id |> devices_of(now) |> Enum.reject(&(&1 == exclude))

    {:ok, Enum.flat_map(devices, &claim_one(account_id, &1, now))}
  end

  defp devices_of(account_id, now) do
    Repo.all(
      from k in KeyPackage,
        where: k.account_id == ^account_id and k.expires_at > ^now,
        distinct: true,
        order_by: k.device_id,
        select: k.device_id
    )
  end

  defp claim_one(account_id, device_id, now) do
    pool =
      from k in KeyPackage,
        where:
          k.account_id == ^account_id and k.device_id == ^device_id and not k.last_resort and
            k.expires_at > ^now,
        order_by: k.inserted_at,
        limit: 1,
        lock: "FOR UPDATE SKIP LOCKED"

    case Repo.one(pool) do
      %KeyPackage{} = kp ->
        Repo.delete!(kp)
        [%{device_id: device_id, key_package: kp.key_package}]

      nil ->
        last_resort(account_id, device_id, now)
    end
  end

  defp last_resort(account_id, device_id, now) do
    query =
      from k in KeyPackage,
        where:
          k.account_id == ^account_id and k.device_id == ^device_id and k.last_resort and
            k.expires_at > ^now,
        select: k.key_package

    case Repo.one(query) do
      nil -> []
      bytes -> [%{device_id: device_id, key_package: bytes}]
    end
  end

  @doc "Deletes every package of a device: on revocation (e2ee spec section 3)."
  def purge_device(account_id, device_id) do
    {n, _} =
      Repo.delete_all(
        from k in KeyPackage, where: k.account_id == ^account_id and k.device_id == ^device_id
      )

    n
  end

  @doc "Deletes expired packages. Returns the count."
  def purge_expired do
    now = DateTime.utc_now()
    {n, _} = Repo.delete_all(from k in KeyPackage, where: k.expires_at < ^now)
    n
  end

  # ---- GroupInfo -----------------------------------------------------------------

  @doc """
  Keeps the newest GroupInfo for a conversation by `epoch`; an older or
  equal epoch is a no-op. Participants only, else `{:error, :not_found}`.
  """
  def put_group_info(channel_id, account_id, epoch, bytes)
      when is_integer(epoch) and is_binary(bytes) do
    if Dms.participant?(channel_id, account_id) do
      now = DateTime.utc_now()

      # Replaced only when strictly newer; otherwise the insert is a no-op.
      newer =
        from g in GroupInfo,
          where: g.epoch < ^epoch,
          update: [set: [epoch: ^epoch, group_info: ^bytes, updated_at: ^now]]

      # insert_all rather than insert: a conditional on_conflict that does
      # not fire returns no row, which insert reports as a stale entry.
      Repo.insert_all(
        GroupInfo,
        [%{channel_id: channel_id, epoch: epoch, group_info: bytes, updated_at: now}],
        on_conflict: newer,
        conflict_target: :channel_id
      )

      :ok
    else
      {:error, :not_found}
    end
  end

  @doc "The newest GroupInfo for a participant: `{:ok, %{group_info, epoch}}` or `{:error, :not_found}`."
  def get_group_info(channel_id, account_id) do
    with true <- Dms.participant?(channel_id, account_id),
         %GroupInfo{} = gi <- Repo.get(GroupInfo, channel_id) do
      {:ok, %{group_info: gi.group_info, epoch: gi.epoch}}
    else
      _ -> {:error, :not_found}
    end
  end
end
