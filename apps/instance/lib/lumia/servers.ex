defmodule Lumia.Servers do
  @moduledoc """
  Servers, memberships, roles, and overwrites, with the propagation rules
  from docs/specs/authorization.md section 6:

    * definition-level changes (roles, overwrites, channels, ownership) bump
      `acl_version` on every affected channel in the same transaction and
      schedule `AclChanged` events and a viewer diff;
    * member-level changes (role assignment, kick, join) do not touch
      `acl_version`; they append to the affected member's account log.

  Every change writes an audit row in the same transaction. Authorization
  checks (who may do what) live here too, built on `Lumia.Authz`.
  """

  import Bitwise
  import Ecto.Query

  alias Lumia.Accounts
  alias Lumia.Audit
  alias Lumia.Authz
  alias Lumia.Authz.Permission
  alias Lumia.Events.Channel
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers.ChannelMember
  alias Lumia.Servers.ChannelOverwrite
  alias Lumia.Servers.Member
  alias Lumia.Servers.MemberRole
  alias Lumia.Servers.Role
  alias Lumia.Servers.Server
  alias Lumia.Tx
  alias Lumia.V1
  alias Lumia.Workers.AclChanged
  alias Lumia.Workers.ServerUpdated
  alias Lumia.Workers.ViewerDiff

  @everyone_defaults [
    :view_channel,
    :read_history,
    :send_messages,
    :send_in_threads,
    :attach_files,
    :add_reactions,
    :start_calls,
    :start_server_dms
  ]

  @name_max 100

  # ---- Servers ---------------------------------------------------------------

  @doc """
  A server's name as kept: trimmed, 1 to #{@name_max} characters
  (architecture 6.7), or `{:error, {:invalid_argument, "name"}}`.
  """
  def server_name(name) when is_binary(name) do
    name = String.trim(name)
    length = String.length(name)

    if length > 0 and length <= @name_max,
      do: {:ok, name},
      else: {:error, {:invalid_argument, "name"}}
  end

  def server_name(_), do: {:error, {:invalid_argument, "name"}}

  @doc "Creates a server with its @everyone role and the owner's membership."
  def create_server(owner_account_id, name) do
    with {:ok, name} <- server_name(name) do
      insert_server(owner_account_id, name)
    end
  end

  defp insert_server(owner_account_id, name) do
    Tx.transact(fn ->
      with {:ok, server} <- Repo.insert(%Server{name: name, owner_account_id: owner_account_id}),
           {:ok, _everyone} <-
             Repo.insert(%Role{
               server_id: server.id,
               name: "@everyone",
               position: 1_000_000,
               permissions: Permission.to_bits(@everyone_defaults),
               is_everyone: true
             }),
           {:ok, _} <- insert_member(server.id, owner_account_id, "member"),
           {:ok, _} <-
             Audit.record({:server, server.id}, owner_account_id, "server.create",
               after: %{"name" => name}
             ) do
        {:ok, server}
      end
    end)
  end

  @doc """
  Creates a text channel in a server; the actor needs `manage_channels`
  there (the owner has everything). Bumps nothing; the channel is new at
  version 1. With `private: true` its `@everyone` overwrite denies view, so
  only those in it see it (architecture 6.8). The creator is in it from
  the start; a server's first channel is its default.
  """
  def create_channel(server_id, actor_id, name, opts \\ []) do
    private = Keyword.get(opts, :private, false)

    Tx.transact(fn ->
      with {:ok, actor} <- actor(server_id, actor_id),
           :ok <- require_permission(actor, :manage_channels),
           first? = not Repo.exists?(from c in Channel, where: c.server_id == ^server_id),
           {:ok, channel} <-
             Repo.insert(%Channel{
               kind: "text",
               server_id: server_id,
               name: name,
               topic: Keyword.get(opts, :topic),
               position: Keyword.get(opts, :position, 0),
               private: private,
               is_default: first? and not private
             }),
           :ok <- hide_from_everyone(channel, private),
           {:ok, _} <- Lumia.Channels.enter_quietly(channel, actor_id),
           {:ok, _} <-
             Audit.record({:server, server_id}, actor_id, "channel.create",
               target: {:channel, channel.id},
               after: %{"name" => name}
             ),
           {:ok, _} <- ViewerDiff.schedule(server_id) do
        {:ok, channel}
      end
    end)
  end

  defp hide_from_everyone(_channel, false), do: :ok

  defp hide_from_everyone(channel, true) do
    everyone =
      Repo.one!(
        from r in Role, where: r.server_id == ^channel.server_id and r.is_everyone, select: r.id
      )

    with {:ok, _} <-
           Repo.insert(%ChannelOverwrite{
             channel_id: channel.id,
             target_kind: "role",
             target_id: everyone,
             allow: 0,
             deny: Permission.bit(:view_channel)
           }),
         do: :ok
  end

  # ---- Membership (member-level: account log, no acl bump) --------------------

  @doc """
  Adds a member or guest. Appends MembershipAdded to their account log,
  and they join the server's default channels and the one `channel_id`
  names, if any, as far as they can see them (architecture 6.8).
  """
  def add_member(server_id, account_id, actor_id, opts \\ []) do
    kind = Keyword.get(opts, :kind, "member")

    Tx.transact(fn ->
      with {:ok, member} <-
             insert_member(server_id, account_id, kind, Keyword.get(opts, :expires_at)),
           {:ok, server} <- fetch_server(server_id),
           {:ok, _} <-
             Accounts.append_event(
               account_id,
               {:membership_added,
                %V1.MembershipAdded{member: Proto.member(member), server: Proto.server(server)}}
             ),
           :ok <- Lumia.Channels.arrive(server_id, account_id, Keyword.get(opts, :channel_id)),
           {:ok, _} <-
             Audit.record({:server, server_id}, actor_id, "member.add",
               target: {:account, account_id},
               after: %{"kind" => kind}
             ),
           {:ok, _} <- ViewerDiff.schedule(server_id) do
        {:ok, member}
      end
    end)
  end

  @doc """
  Removes a member (kick, leave, expiry). The owner cannot be removed. A
  kick requires the actor to outrank the target (section 3.3); `reason`
  is `"left"`, `"removed"`, `"guest_expired"`, or `"account_deleted"`.
  """
  def remove_member(server_id, account_id, actor_id, reason) do
    Tx.transact(fn ->
      with {:ok, server} <- fetch_server(server_id),
           :ok <- check_removal(server, account_id, actor_id, reason),
           {1, _} <- delete_member(server_id, account_id),
           :ok <- Lumia.Rails.forget(account_id, server_id),
           :ok <- leave_channels(server_id, account_id),
           channel_ids = viewable_channel_ids(server_id, account_id),
           :ok <- forget_viewer(account_id, channel_ids),
           {:ok, _} <-
             Accounts.append_event(
               account_id,
               {:membership_removed,
                %V1.MembershipRemoved{
                  server_id: Proto.uuid(server_id),
                  reason: Proto.leave_reason(reason)
                }}
             ),
           :ok <- append_channel_removed(account_id, channel_ids),
           {:ok, _} <-
             Audit.record({:server, server_id}, actor_id, "member.remove",
               target: {:account, account_id},
               after: %{"reason" => reason}
             ),
           {:ok, _} <- ViewerDiff.schedule(server_id) do
        {:ok, :ok}
      else
        {0, _} -> {:error, :not_found}
        other -> other
      end
    end)
    |> unwrap()
  end

  @doc """
  The member's own switches in a server (architecture 6.8):
  `:channel_adds_by_invitation`, `:server_dms_off`, either or both. Their
  other devices hear of it through `MembershipUpdated`. Returns the member.
  """
  def update_membership(server_id, account_id, changes) do
    Tx.transact(fn ->
      with {:ok, member} <- fetch_member(server_id, account_id),
           {:ok, updated} <- member |> Ecto.Changeset.change(changes) |> Repo.update(),
           {:ok, _} <-
             Accounts.append_event(
               account_id,
               {:membership_updated, %V1.MembershipUpdated{member: own_member(updated)}}
             ) do
        {:ok, updated}
      end
    end)
  end

  @doc "Assigns a role. Member-level: MembershipUpdated on the account log, no acl bump."
  def assign_role(server_id, account_id, role_id, actor_id) do
    change_roles(server_id, account_id, role_id, actor_id, :assign)
  end

  @doc "Removes a role. Member-level: MembershipUpdated on the account log, no acl bump."
  def unassign_role(server_id, account_id, role_id, actor_id) do
    change_roles(server_id, account_id, role_id, actor_id, :unassign)
  end

  defp change_roles(server_id, account_id, role_id, actor_id, op) do
    Tx.transact(fn ->
      with {:ok, role} <- fetch_role(server_id, role_id),
           {:ok, actor} <- actor(server_id, actor_id),
           true <- Authz.can_manage_role?(actor, role) || {:error, :permission_denied},
           {:ok, _} <- apply_role_change(server_id, account_id, role_id, op),
           {:ok, member} <- fetch_member(server_id, account_id),
           {:ok, _} <-
             Accounts.append_event(
               account_id,
               {:membership_updated, %V1.MembershipUpdated{member: own_member(member)}}
             ),
           {:ok, _} <-
             Audit.record({:server, server_id}, actor_id, "member.#{op}_role",
               target: {:account, account_id},
               after: %{"role_id" => role_id}
             ),
           {:ok, _} <- ViewerDiff.schedule(server_id) do
        {:ok, :ok}
      end
    end)
    |> unwrap()
  end

  @doc """
  Sets a member's roles to exactly `role_ids` in one transaction: roles
  not yet held are assigned, roles no longer wanted are removed, and the
  member's account log gets one `MembershipUpdated`. Every role changed
  must be one the actor can manage (section 3.3); the result respects the
  `roles_per_member` limit (section 10). Returns the member.
  """
  def set_roles(server_id, account_id, role_ids, actor_id) when is_list(role_ids) do
    wanted = Enum.uniq(role_ids)

    Tx.transact(fn ->
      with {:ok, member} <- fetch_member(server_id, account_id),
           {:ok, actor} <- actor(server_id, actor_id),
           current = role_ids(server_id, account_id),
           changes = (wanted -- current) ++ (current -- wanted),
           :ok <- check_manageable(server_id, actor, changes),
           :ok <- check_member_role_limit(length(wanted)),
           :ok <- each(wanted -- current, &apply_role_change(server_id, account_id, &1, :assign)),
           :ok <-
             each(current -- wanted, &apply_role_change(server_id, account_id, &1, :unassign)),
           :ok <- announce_roles(server_id, account_id, actor_id, member, changes) do
        {:ok, member}
      end
    end)
  end

  defp check_manageable(server_id, actor, role_ids) do
    each(role_ids, &manageable(server_id, actor, &1))
  end

  defp manageable(server_id, actor, role_id) do
    with {:ok, role} <- fetch_role(server_id, role_id),
         true <- Authz.can_manage_role?(actor, role) || {:error, :permission_denied} do
      :ok
    end
  end

  # No change, no event: a PATCH that repeats the current roles is a no-op.
  defp announce_roles(_server_id, _account_id, _actor_id, _member, []), do: :ok

  defp announce_roles(server_id, account_id, actor_id, member, changes) do
    with {:ok, _} <-
           Accounts.append_event(
             account_id,
             {:membership_updated, %V1.MembershipUpdated{member: own_member(member)}}
           ),
         {:ok, _} <-
           Audit.record({:server, server_id}, actor_id, "member.set_roles",
             target: {:account, account_id},
             after: %{"role_ids" => role_ids(server_id, account_id), "changed" => changes}
           ),
         {:ok, _} <- ViewerDiff.schedule(server_id) do
      :ok
    end
  end

  defp each(items, fun) do
    Enum.reduce_while(items, :ok, fn item, :ok ->
      case fun.(item) do
        :ok -> {:cont, :ok}
        {:ok, _} -> {:cont, :ok}
        error -> {:halt, error}
      end
    end)
  end

  defp apply_role_change(server_id, account_id, role_id, :assign) do
    current = role_ids(server_id, account_id)
    limit = if role_id in current, do: :ok, else: check_member_role_limit(length(current) + 1)

    with :ok <- limit do
      %MemberRole{server_id: server_id, account_id: account_id, role_id: role_id}
      |> Ecto.Changeset.change()
      |> Ecto.Changeset.unique_constraint([:server_id, :account_id, :role_id],
        name: :member_roles_pkey
      )
      |> Repo.insert(on_conflict: :nothing)
    end
  end

  defp apply_role_change(server_id, account_id, role_id, :unassign) do
    Repo.delete_all(
      from mr in MemberRole,
        where:
          mr.server_id == ^server_id and mr.account_id == ^account_id and mr.role_id == ^role_id
    )

    {:ok, nil}
  end

  # ---- Roles and overwrites (definition-level: acl bump) ----------------------

  @doc """
  Creates a role. Requires MANAGE_ROLES and that the actor holds every
  permission granted. Bumps every channel in the server: a new role
  changes nothing until assigned, but its position shifts the hierarchy.
  """
  def create_role(server_id, actor_id, attrs) do
    perms = Permission.to_bits(Map.get(attrs, :permissions, []))

    Tx.transact(fn ->
      with {:ok, actor} <- actor(server_id, actor_id),
           true <- Authz.can_grant?(actor, perms) || {:error, :permission_denied},
           true <-
             Permission.has?(actor.server_permissions, :manage_roles) || actor.owner? ||
               {:error, :permission_denied},
           :ok <- check_role_limit(server_id),
           {:ok, role} <-
             Repo.insert(%Role{
               server_id: server_id,
               name: Map.fetch!(attrs, :name),
               position: Map.get(attrs, :position, 0),
               permissions: perms,
               color: Map.get(attrs, :color)
             }),
           {:ok, _} <-
             Audit.record({:server, server_id}, actor_id, "role.create",
               target: {:role, role.id},
               after: role_json(role)
             ),
           {:ok, _} <- bump_server(server_id) do
        {:ok, role}
      end
    end)
  end

  @doc "Updates a role's permissions, name, position, or color. Bumps every channel in the server."
  def update_role(server_id, role_id, actor_id, attrs) do
    Tx.transact(fn ->
      with {:ok, role} <- fetch_role(server_id, role_id),
           {:ok, actor} <- actor(server_id, actor_id),
           true <-
             (role.is_everyone or Authz.can_manage_role?(actor, role)) ||
               {:error, :permission_denied},
           true <-
             (Permission.has?(actor.server_permissions, :manage_roles) or actor.owner?) ||
               {:error, :permission_denied},
           new_perms = new_permissions(role, attrs),
           true <-
             Authz.can_grant?(actor, new_perms &&& bnot(role.permissions)) ||
               {:error, :permission_denied},
           {:ok, updated} <-
             role
             |> Ecto.Changeset.change(
               name: Map.get(attrs, :name, role.name),
               position: Map.get(attrs, :position, role.position),
               permissions: new_perms,
               color: Map.get(attrs, :color, role.color)
             )
             |> Repo.update(),
           {:ok, _} <-
             Audit.record({:server, server_id}, actor_id, "role.update",
               target: {:role, role.id},
               before: role_json(role),
               after: role_json(updated)
             ),
           {:ok, _} <- bump_server(server_id) do
        {:ok, updated}
      end
    end)
  end

  @doc "Deletes a role. @everyone cannot be deleted. Bumps every channel in the server."
  def delete_role(server_id, role_id, actor_id) do
    Tx.transact(fn ->
      with {:ok, role} <- fetch_role(server_id, role_id),
           :ok <- deletable(role),
           {:ok, actor} <- actor(server_id, actor_id),
           true <- Authz.can_manage_role?(actor, role) || {:error, :permission_denied},
           {:ok, _} <- Repo.delete(role),
           {:ok, _} <-
             Audit.record({:server, server_id}, actor_id, "role.delete",
               target: {:role, role.id},
               before: role_json(role)
             ),
           {:ok, _} <- bump_server(server_id) do
        {:ok, :ok}
      end
    end)
    |> unwrap()
  end

  @doc """
  Sets a channel overwrite for a role or member. Requires MANAGE_CHANNELS
  in the server and that the actor holds every permission in `allow`.
  Normalized so `allow` and `deny` never overlap. Bumps that channel.
  """
  def set_overwrite(channel_id, actor_id, {target_kind, target_id}, allow, deny) do
    ow =
      Authz.normalize_overwrite(%{
        allow: Permission.to_bits(allow),
        deny: Permission.to_bits(deny)
      })

    Tx.transact(fn ->
      with {:ok, channel} <- fetch_server_channel(channel_id),
           {:ok, actor} <- actor(channel.server_id, actor_id),
           true <-
             (Permission.has?(actor.server_permissions, :manage_channels) or actor.owner?) ||
               {:error, :permission_denied},
           true <- Authz.can_grant?(actor, ow.allow) || {:error, :permission_denied},
           :ok <- check_overwrite_target(channel.server_id, target_kind, target_id),
           :ok <- check_overwrite_limit(channel_id),
           {:ok, _} <-
             Repo.insert(
               %ChannelOverwrite{
                 channel_id: channel_id,
                 target_kind: to_string(target_kind),
                 target_id: target_id,
                 allow: ow.allow,
                 deny: ow.deny
               },
               on_conflict: {:replace, [:allow, :deny]},
               conflict_target: [:channel_id, :target_kind, :target_id]
             ),
           {:ok, _} <-
             Audit.record({:server, channel.server_id}, actor_id, "overwrite.set",
               target: {target_kind, target_id},
               after: %{
                 "channel_id" => channel_id,
                 "allow" => Permission.from_bits(ow.allow),
                 "deny" => Permission.from_bits(ow.deny)
               }
             ),
           {:ok, _} <- bump_channel(channel_id, channel.server_id) do
        {:ok, :ok}
      end
    end)
    |> unwrap()
  end

  # ---- Governance policy ----------------------------------------------------------

  @doc """
  Sets a server's retention in days (nil for forever). MANAGE_SERVER.
  Governance policy, not an entitlement (architecture 16): no plan can
  override it.
  """
  def set_retention(server_id, actor_id, days)
      when is_nil(days) or (is_integer(days) and days > 0),
      do: update_server(server_id, actor_id, %{retention_days: days || 0})

  # ---- Settings (architecture 6.7) --------------------------------------------

  @doc """
  Changes a server's settings: `:name`, `:retention_days` (0 keeps
  messages for ever), `:show_deleted`, any of them; a key left out is
  left as it is.
  Needs `MANAGE_SERVER` (the owner and `ADMINISTRATOR` have it). Each
  setting changed is audited, and every member is told (`ServerUpdated`).
  """
  def update_server(server_id, actor_id, changes) do
    with {:ok, changes} <- settings(changes),
         {:ok, {_before, updated}} <- change_server(server_id, actor_id, changes) do
      {:ok, updated}
    end
  end

  @doc """
  Makes `upload` (a picture's bytes, as chosen) the server's picture, as
  `Lumia.Accounts.set_avatar/2` does an account's: the instance's own
  square, kept as a blob and named in `icon_url`. The picture it replaces
  is removed. `MANAGE_SERVER`; every member is told.
  """
  def set_icon(server_id, actor_id, upload) when is_binary(upload) do
    with :ok <- may_manage(server_id, actor_id),
         true <- Lumia.Blobs.enabled?() || {:error, {:failed_precondition, "blobs"}},
         {:ok, {media_type, bytes}} <- Lumia.Media.avatar(upload),
         {:ok, blob} <- Lumia.Blobs.put(bytes, "avatar", media_type, actor_id) do
      case put_icon(server_id, actor_id, Accounts.picture_path(blob)) do
        {:ok, server} ->
          {:ok, server}

        error ->
          Lumia.Blobs.delete(blob.address)
          error
      end
    else
      {:error, :unavailable} -> {:error, {:failed_precondition, "blobs"}}
      other -> other
    end
  end

  @doc "Takes the server's picture away; fine when it has none. `MANAGE_SERVER`."
  def remove_icon(server_id, actor_id), do: put_icon(server_id, actor_id, nil)

  defp put_icon(server_id, actor_id, url) do
    with {:ok, {before, updated}} <- change_server(server_id, actor_id, %{icon_url: url}) do
      drop_icon(before.icon_url, updated.icon_url)
      {:ok, updated}
    end
  end

  # The picture the server no longer names is nobody's.
  defp drop_icon(same, same), do: :ok

  defp drop_icon(old, _new) do
    if address = Accounts.avatar_address(old), do: Lumia.Blobs.delete(address)
    :ok
  end

  # The change, checked, made, audited and announced in one transaction:
  # `{:ok, {before, after}}`.
  defp change_server(server_id, actor_id, changes) do
    Tx.transact(fn ->
      with {:ok, server} <- fetch_server(server_id),
           :ok <- may_manage(server_id, actor_id),
           {:ok, updated} <- server |> Ecto.Changeset.change(changes) |> Repo.update(),
           :ok <- announce(server, updated, actor_id) do
        {:ok, {server, updated}}
      end
    end)
  end

  defp may_manage(server_id, actor_id) do
    with {:ok, actor} <- actor(server_id, actor_id) do
      if Permission.has?(actor.server_permissions, :manage_server) or actor.owner?,
        do: :ok,
        else: {:error, :permission_denied}
    end
  end

  defp settings(changes) do
    Enum.reduce_while(changes, {:ok, %{}}, fn
      {:name, name}, {:ok, acc} ->
        case server_name(name) do
          {:ok, name} -> {:cont, {:ok, Map.put(acc, :name, name)}}
          error -> {:halt, error}
        end

      {:retention_days, 0}, {:ok, acc} ->
        {:cont, {:ok, Map.put(acc, :retention_days, nil)}}

      {:retention_days, days}, {:ok, acc} when is_integer(days) and days > 0 ->
        {:cont, {:ok, Map.put(acc, :retention_days, days)}}

      {:show_deleted, show}, {:ok, acc} when is_boolean(show) ->
        {:cont, {:ok, Map.put(acc, :show_deleted, show)}}

      {key, _}, _ ->
        {:halt, {:error, {:invalid_argument, Atom.to_string(key)}}}
    end)
  end

  # Each setting that changed is audited, and the members are to hear of it.
  defp announce(before, after_change, actor_id) do
    changed =
      for {key, action} <- [
            name: "server.name",
            retention_days: "server.retention",
            show_deleted: "server.show_deleted",
            icon_url: "server.icon"
          ],
          Map.get(before, key) != Map.get(after_change, key),
          do: {key, action}

    Enum.each(changed, fn {key, action} ->
      {:ok, _} =
        Audit.record({:server, before.id}, actor_id, action,
          before: %{Atom.to_string(key) => Map.get(before, key)},
          after: %{Atom.to_string(key) => Map.get(after_change, key)}
        )
    end)

    if changed != [], do: {:ok, _} = ServerUpdated.schedule(before.id)
    :ok
  end

  # ---- Resolution helpers for callers -----------------------------------------

  @doc "The member map `Lumia.Authz.resolve/2` takes (account_id, kind, role_ids, owner?), or nil if not a member."
  def member_for(server_id, account_id) do
    case Repo.get_by(Member, server_id: server_id, account_id: account_id) do
      nil ->
        nil

      %Member{kind: kind} ->
        %{
          account_id: account_id,
          kind: String.to_existing_atom(kind),
          role_ids: role_ids(server_id, account_id),
          owner?: owner?(server_id, account_id)
        }
    end
  end

  @doc """
  Every account the channel is shown to, resolved fresh from Postgres:
  those in it who are still members of its server and can view it
  (architecture 6.8). The viewer diff announces this set's changes.
  """
  def viewers(channel_id) do
    with {:ok, snapshot} when is_binary(snapshot.server_id) <-
           Lumia.Authz.Cache.fetch_current(channel_id),
         %Channel{} = channel <- Repo.get(Channel, channel_id) do
      members =
        Repo.all(
          from cm in ChannelMember,
            join: m in Member,
            on: m.server_id == ^snapshot.server_id and m.account_id == cm.account_id,
            where: cm.channel_id == ^channel_id,
            select: {m.account_id, m.kind}
        )

      roles_by_account =
        Repo.all(
          from mr in MemberRole,
            where: mr.server_id == ^snapshot.server_id,
            select: {mr.account_id, mr.role_id}
        )

      role_map = Enum.group_by(roles_by_account, &elem(&1, 0), &elem(&1, 1))

      for {account_id, kind} <- members,
          member = %{
            account_id: account_id,
            kind: String.to_existing_atom(kind),
            role_ids: Map.get(role_map, account_id, []),
            owner?: snapshot.owner_account_id == account_id
          },
          bits = Lumia.Authz.Access.permissions(snapshot, member, channel, true),
          Permission.has?(bits, :view_channel),
          do: account_id
    else
      _ -> []
    end
  end

  # ---- Internals ---------------------------------------------------------------

  # A membership as its own account's log carries it: with its roles and
  # its resolved server-level permissions, as the snapshot gives them, so a
  # client knows what to offer after a change as it did before one.
  defp own_member(%Member{server_id: server_id, account_id: account_id} = member) do
    permissions =
      case actor(server_id, account_id) do
        {:ok, actor} -> actor.server_permissions
        _ -> 0
      end

    Proto.member(member, role_ids(server_id, account_id), permissions)
  end

  defp insert_member(server_id, account_id, kind, expires_at \\ nil) do
    %Member{
      server_id: server_id,
      account_id: account_id,
      kind: kind,
      joined_at: DateTime.utc_now(),
      expires_at: expires_at
    }
    |> Ecto.Changeset.change()
    |> Ecto.Changeset.unique_constraint([:server_id, :account_id], name: :server_members_pkey)
    |> Repo.insert()
  end

  defp delete_member(server_id, account_id) do
    Repo.delete_all(
      from m in Member, where: m.server_id == ^server_id and m.account_id == ^account_id
    )
  end

  defp check_removal(server, account_id, _actor_id, _reason)
       when server.owner_account_id == account_id, do: {:error, :permission_denied}

  defp check_removal(_server, account_id, account_id, "left"), do: :ok

  defp check_removal(_server, _account_id, nil, reason)
       when reason in ~w(guest_expired account_deleted), do: :ok

  defp check_removal(server, account_id, actor_id, "removed") do
    with {:ok, actor} <- actor(server.id, actor_id) do
      target = %{owner?: false, highest_position: highest_position(server.id, account_id)}
      if Authz.can_remove_member?(actor, target), do: :ok, else: {:error, :permission_denied}
    end
  end

  defp check_removal(_server, _account_id, _actor_id, _reason), do: {:error, :permission_denied}

  # Out of every channel of the server and every invitation into one; a
  # server DM stays theirs, hidden while they are not a member.
  defp leave_channels(server_id, account_id) do
    text = from c in Channel, where: c.server_id == ^server_id and c.kind == "text", select: c.id

    Repo.delete_all(
      from m in ChannelMember,
        where: m.account_id == ^account_id and m.channel_id in subquery(text)
    )

    Repo.delete_all(
      from i in Lumia.Servers.ChannelInvitation,
        where: i.account_id == ^account_id and i.channel_id in subquery(text)
    )

    :ok
  end

  defp forget_viewer(account_id, channel_ids) do
    Repo.delete_all(
      from v in Lumia.Servers.ChannelViewer,
        where: v.account_id == ^account_id and v.channel_id in ^channel_ids
    )

    :ok
  end

  defp viewable_channel_ids(server_id, account_id) do
    Repo.all(
      from v in "channel_viewers",
        join: c in Channel,
        on: c.id == v.channel_id,
        where: c.server_id == ^server_id and v.account_id == type(^account_id, Ecto.UUID),
        select: type(v.channel_id, Ecto.UUID)
    )
  end

  defp append_channel_removed(account_id, channel_ids) do
    Enum.reduce_while(channel_ids, :ok, fn channel_id, :ok ->
      case Accounts.append_event(
             account_id,
             {:channel_removed, %V1.ChannelRemoved{channel_id: Proto.uuid(channel_id)}}
           ) do
        {:ok, _} -> {:cont, :ok}
        error -> {:halt, error}
      end
    end)
  end

  defp fetch_server(server_id) do
    case Repo.get(Server, server_id),
      do: (
        nil -> {:error, :not_found}
        s -> {:ok, s}
      )
  end

  defp fetch_member(server_id, account_id) do
    case Repo.get_by(Member, server_id: server_id, account_id: account_id),
      do: (
        nil -> {:error, :not_found}
        m -> {:ok, m}
      )
  end

  defp fetch_role(server_id, role_id) do
    case Repo.get_by(Role, id: role_id, server_id: server_id),
      do: (
        nil -> {:error, :not_found}
        r -> {:ok, r}
      )
  end

  defp fetch_channel(channel_id) do
    case Repo.get(Channel, channel_id),
      do: (
        nil -> {:error, :not_found}
        c -> {:ok, c}
      )
  end

  # Overwrites belong to server channels; a channel with no server (a
  # personal DM, a spike-level channel) has no roles to overwrite.
  defp fetch_server_channel(channel_id) do
    case fetch_channel(channel_id) do
      {:ok, %Channel{server_id: nil}} -> {:error, :not_found}
      other -> other
    end
  end

  # The target of an overwrite must exist in this server: a role of the
  # server, or a member of it. Anything else names nothing here.
  defp check_overwrite_target(server_id, :role, role_id) when is_binary(role_id) do
    with {:ok, _role} <- fetch_role(server_id, role_id), do: :ok
  end

  defp check_overwrite_target(server_id, :member, account_id) when is_binary(account_id) do
    with {:ok, _member} <- fetch_member(server_id, account_id), do: :ok
  end

  defp check_overwrite_target(_server_id, _kind, _id), do: {:error, :not_found}

  defp check_member_role_limit(count) do
    if count <= limit(:roles_per_member), do: :ok, else: {:error, :limit_exceeded}
  end

  defp role_ids(server_id, account_id) do
    Repo.all(
      from mr in MemberRole,
        where: mr.server_id == ^server_id and mr.account_id == ^account_id,
        select: mr.role_id
    )
  end

  defp owner?(server_id, account_id) do
    Repo.exists?(
      from s in Server, where: s.id == ^server_id and s.owner_account_id == ^account_id
    )
  end

  defp highest_position(server_id, account_id) do
    Repo.one(
      from mr in MemberRole,
        join: r in Role,
        on: r.id == mr.role_id,
        where: mr.server_id == ^server_id and mr.account_id == ^account_id,
        select: min(r.position)
    )
  end

  # The actor's standing for management checks: server-level permissions
  # are resolved with no channel overwrites, which is what "at the server
  # level" means in the spec.
  @doc """
  The account as an actor in the server: whether it owns the server, its
  resolved server-level permissions and its highest role position;
  `{:error, :permission_denied}` when it is not a member. Invites check
  their permission through this.
  """
  def actor(server_id, account_id) do
    case member_for(server_id, account_id) do
      nil ->
        {:error, :permission_denied}

      member ->
        roles = Repo.all(from r in Role, where: r.server_id == ^server_id)
        everyone = Enum.find(roles, & &1.is_everyone)

        snapshot = %Authz.Snapshot{
          server_id: server_id,
          everyone_role_id: everyone && everyone.id,
          roles: Map.new(roles, &{&1.id, %{permissions: &1.permissions, position: &1.position}}),
          overwrites: %{}
        }

        {:ok,
         %{
           account_id: account_id,
           owner?: member.owner?,
           server_permissions: Authz.resolve(snapshot, member),
           highest_position: highest_position(server_id, account_id)
         }}
    end
  end

  defp require_permission(%{owner?: true}, _permission), do: :ok

  defp require_permission(actor, permission) do
    if Permission.has?(actor.server_permissions, permission),
      do: :ok,
      else: {:error, :permission_denied}
  end

  defp bump_server(server_id) do
    {_, _} =
      Repo.update_all(from(c in Channel, where: c.server_id == ^server_id), inc: [acl_version: 1])

    with {:ok, _} <- AclChanged.schedule(server_id) do
      ViewerDiff.schedule(server_id)
    end
  end

  defp bump_channel(channel_id, server_id) do
    {1, [acl_version]} =
      Repo.update_all(from(c in Channel, where: c.id == ^channel_id, select: c.acl_version),
        inc: [acl_version: 1]
      )

    with {:ok, _} <-
           Lumia.Events.append_system(
             channel_id,
             {:acl_changed, %V1.AclChanged{acl_version: acl_version}}
           ) do
      ViewerDiff.schedule(server_id)
    end
  end

  defp check_role_limit(server_id) do
    if Repo.aggregate(from(r in Role, where: r.server_id == ^server_id), :count) <
         limit(:roles_per_server),
       do: :ok,
       else: {:error, :limit_exceeded}
  end

  defp check_overwrite_limit(channel_id) do
    if Repo.aggregate(from(o in ChannelOverwrite, where: o.channel_id == ^channel_id), :count) <
         limit(:overwrites_per_channel),
       do: :ok,
       else: {:error, :limit_exceeded}
  end

  # Operator hard limits (authorization section 10, architecture 16).
  defp limit(key) do
    Application.get_env(:lumia, :limits, [])
    |> Keyword.get(
      key,
      %{roles_per_server: 50, overwrites_per_channel: 50, roles_per_member: 20}[key]
    )
  end

  defp new_permissions(_role, %{permissions: names}) when is_list(names),
    do: Permission.to_bits(names)

  defp new_permissions(role, _attrs), do: role.permissions

  defp deletable(%Role{is_everyone: true}), do: {:error, :permission_denied}
  defp deletable(%Role{}), do: :ok

  # Tx.transact/1 requires {:ok, _} | {:error, _}; callers want :ok.
  defp unwrap({:ok, :ok}), do: :ok
  defp unwrap(other), do: other

  defp role_json(%Role{} = r) do
    %{
      "name" => r.name,
      "position" => r.position,
      "permissions" => Permission.from_bits(r.permissions),
      "color" => r.color
    }
  end
end
