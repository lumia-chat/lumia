defmodule Lumia.Uploads do
  @moduledoc """
  Attachment blobs (specs/media.md 4). A client encrypts each blob itself
  and sends it straight to the blob store through a URL this module
  presigns (or, for a directory store, the instance's own signed URL), so
  the instance never sees an attachment in the clear and its bytes never
  pass through a node on their way to a bucket. What the instance keeps is
  a row per blob: who uploaded it, for which conversation and role, how
  long it is, and the message that named it.

  A blob is fetched through `fetch/2`, which checks that the caller can
  read the message naming it; its id is a storage key, never an
  authorization. Blobs no message named within a day, and those of
  messages purged, are removed by `sweep/0`.
  """

  import Ecto.Query

  alias Lumia.Authz.Access
  alias Lumia.Blobs.Store
  alias Lumia.Dms
  alias Lumia.Events.Message
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers.Server
  alias Lumia.Tx
  alias Lumia.Uploads.Upload
  alias Lumia.V1

  @tag_bytes 16
  @thumbnail_max 512 * 1024
  @put_seconds 15 * 60
  # Long enough to start a download, short enough that a deleted message's
  # blob stops being reachable within a minute.
  @get_seconds 60
  @orphan_hours 24
  @deleted_days 30
  @max_attachments 10
  @max_dm_blobs 2 * @max_attachments + 3
  @max_name_bytes 255
  @max_alt_chars 1_000
  @max_media_type_bytes 255
  # A ThumbHash is about 25 bytes.
  @max_placeholder_bytes 64

  @doc "Whether attachments can be kept here: the instance has a blob store."
  def enabled?, do: Store.configured() != nil

  @doc """
  The largest blob of each role, in bytes of ciphertext: `%{image:, file:,
  thumbnail:}` (specs/media.md 2), from `LUMIA_UPLOAD_IMAGE_MAX_BYTES` and
  `LUMIA_UPLOAD_FILE_MAX_BYTES`.
  """
  def limits do
    config = Application.get_env(:lumia, :uploads, [])

    %{
      image: Keyword.get(config, :image_max_bytes, 2 * 1024 * 1024),
      file: Keyword.get(config, :file_max_bytes, 10 * 1024 * 1024),
      thumbnail: @thumbnail_max
    }
  end

  @doc "The limits as `GET /instance` names them, or nil when nothing can be attached."
  def limits_proto do
    if enabled?() do
      limits = limits()

      %V1.UploadLimits{
        image_max_bytes: limits.image,
        file_max_bytes: limits.file,
        thumbnail_max_bytes: limits.thumbnail
      }
    end
  end

  @doc """
  A slot for a blob of `role` (`:BLOB_ROLE_DISPLAY`, `:BLOB_ROLE_THUMBNAIL`
  or `:BLOB_ROLE_FILE`) and exactly `size` bytes, in a conversation where
  the account holds `ATTACH_FILES`. `{:ok, %{upload:, url:, headers:,
  expires_at:}}`; `url` is absolute for a bucket and a path under the
  instance for a directory store. Errors: `{:failed_precondition,
  "blobs"}` with no blob store, `{:payload_too_large, max}`,
  `{:resource_exhausted, "storage", max}` when the server's files would
  pass its limit (specs/media.md 4.5), `:invalid_argument`, or the
  channel's access error.
  """
  def create(account_id, channel_id, role, size) when is_integer(size) do
    with true <- enabled?() || {:error, {:failed_precondition, "blobs"}},
         {:ok, role} <- role(role),
         :ok <- within(role, size),
         {:ok, access} <- Access.require(account_id, channel_id, :attach_files),
         now = DateTime.utc_now(),
         {:ok, upload} <- insert(account_id, access.server_id, channel_id, role, size, now) do
      url =
        case Store.presign(object(upload.id), :put, @put_seconds, size) do
          {:ok, url} -> url
          :instance -> "/api/v1/blobs/upload/" <> token(upload)
        end

      {:ok,
       %{
         upload: upload,
         url: url,
         headers: %{
           "content-length" => Integer.to_string(size),
           "content-type" => "application/octet-stream"
         },
         expires_at: DateTime.add(now, @put_seconds, :second)
       }}
    end
  end

  # The slot's row, its bytes counted against the server first.
  defp insert(account_id, server_id, channel_id, role, size, now) do
    Tx.transact(fn ->
      with :ok <- reserve(server_id, size) do
        {:ok,
         Repo.insert!(%Upload{
           uploader_account_id: account_id,
           channel_id: channel_id,
           server_id: server_id,
           role: role,
           size: size,
           created_at: now
         })}
      end
    end)
  end

  # ---- A server's storage (specs/media.md 4.5) ---------------------------------

  @doc "The most a server's files may take, in bytes, or nil for no limit."
  def server_max_bytes, do: Application.get_env(:lumia, :uploads, [])[:server_storage_max_bytes]

  # Counts `size` more bytes against the server, unless that takes it past
  # its limit: then, if the server says so, its oldest attachments expire
  # to make room first. A personal conversation's blob counts against
  # nobody yet.
  defp reserve(nil, _size), do: :ok

  defp reserve(server_id, size) do
    max = server_max_bytes()

    cond do
      add(server_id, size, max) ->
        :ok

      size <= max and storage_full(server_id) == "delete_oldest" and
        make_room(server_id, size, max) and add(server_id, size, max) ->
        :ok

      true ->
        {:error, {:resource_exhausted, "storage", max}}
    end
  end

  # Adds `size` to the server's count if it stays within `max`, in one
  # statement, so two uploads at once cannot both pass the limit.
  defp add(server_id, size, nil) do
    {1, _} =
      Repo.update_all(from(s in Server, where: s.id == ^server_id), inc: [stored_bytes: size])

    true
  end

  defp add(server_id, size, max) do
    {n, _} =
      Repo.update_all(
        from(s in Server, where: s.id == ^server_id and s.stored_bytes + ^size <= ^max),
        inc: [stored_bytes: size]
      )

    n == 1
  end

  defp storage_full(server_id),
    do: Repo.one(from s in Server, where: s.id == ^server_id, select: s.storage_full)

  # Expires the attachments of the server's oldest messages, a stretch of
  # each channel's log at a time, until `size` more bytes fit. Holds the
  # server's row while it does, so uploads making room do not each expire
  # their own. Whether enough was freed.
  defp make_room(server_id, size, limit) do
    used =
      Repo.one(
        from s in Server, where: s.id == ^server_id, lock: "FOR UPDATE", select: s.stored_bytes
      )

    need = used + size - limit

    oldest =
      Repo.all(
        from u in Upload,
          join: m in Message,
          on: m.id == u.message_id and m.channel_id == u.channel_id,
          where: u.server_id == ^server_id and not u.deleting,
          order_by: [asc: m.created_at],
          select: {m.channel_id, m.seq, u.size},
          limit: 10_000
      )

    {through, freed} =
      Enum.reduce_while(oldest, {%{}, 0}, fn {channel_id, seq, size}, {through, freed} ->
        if freed >= need,
          do: {:halt, {through, freed}},
          else: {:cont, {Map.update(through, channel_id, seq, &max(&1, seq)), freed + size}}
      end)

    freed >= need and match?({:ok, _}, expire_through(through))
  end

  @doc """
  Expires the attachments of the server's messages sent before `cutoff`,
  its attachment retention (specs/media.md 4.5). `{:ok, n}`, how many
  messages lost theirs.
  """
  def expire_before(server_id, %DateTime{} = cutoff) do
    from(u in Upload,
      join: m in Message,
      on: m.id == u.message_id and m.channel_id == u.channel_id,
      where: u.server_id == ^server_id and not u.deleting and m.created_at < ^cutoff,
      group_by: m.channel_id,
      select: {m.channel_id, max(m.seq)}
    )
    |> Repo.all()
    |> expire_through()
  end

  # Expires each channel's attachments through the seq given for it.
  defp expire_through(through) do
    Enum.reduce_while(through, {:ok, 0}, fn {channel_id, seq}, {:ok, n} ->
      case Lumia.Events.expire_attachments(channel_id, seq + 1) do
        {:ok, k} -> {:cont, {:ok, n + k}}
        error -> {:halt, error}
      end
    end)
  end

  defp role(:BLOB_ROLE_DISPLAY), do: {:ok, "display"}
  defp role(:BLOB_ROLE_THUMBNAIL), do: {:ok, "thumbnail"}
  defp role(:BLOB_ROLE_FILE), do: {:ok, "file"}
  defp role(_), do: {:error, {:invalid_argument, "role"}}

  defp within(role, size) do
    max = max_for(role)

    cond do
      # An empty file sealed is the tag alone.
      size < @tag_bytes -> {:error, {:invalid_argument, "size_bytes"}}
      size > max -> {:error, {:payload_too_large, max}}
      true -> :ok
    end
  end

  # The blob's object in the store (specs/media.md 4.1).
  defp object(id), do: "att/" <> id

  defp max_for("display"), do: limits().image
  defp max_for("thumbnail"), do: @thumbnail_max
  defp max_for("file"), do: limits().file

  # A directory store's upload URL: a statement of the blob and its length
  # nobody but the instance can make, good for as long as a presigned one.
  defp token(%Upload{id: id, size: size}),
    do: Phoenix.Token.sign(LumiaWeb.Endpoint, "upload", {id, size})

  @doc """
  The blob a directory store's upload token names, for `PUT
  /blobs/upload/{token}`: `{:ok, %Upload{}}` when the token is good and
  the bytes have not arrived yet, else `{:error, :not_found}`.
  """
  def for_token(token) do
    with {:ok, {id, size}} <-
           Phoenix.Token.verify(LumiaWeb.Endpoint, "upload", token, max_age: @put_seconds),
         %Upload{size: ^size, stored: false} = upload <- Repo.get(Upload, id) do
      {:ok, upload}
    else
      _ -> {:error, :not_found}
    end
  end

  @doc """
  Keeps `bytes` as the blob of a directory store's upload: exactly its
  length, or `{:error, {:invalid_argument, "size_bytes"}}`.
  """
  def put_local(%Upload{size: size} = upload, bytes) when byte_size(bytes) == size do
    with :ok <- Store.put(object(upload.id), bytes), do: mark_stored(upload)
  end

  def put_local(%Upload{}, _bytes), do: {:error, {:invalid_argument, "size_bytes"}}

  @doc """
  The client says its blob is in the store: the stored length must be the
  slot's, or the slot and whatever was stored go
  (`{:error, {:invalid_argument, "size_bytes"}}`). `{:error, :not_found}`
  for another's blob, or one whose bytes never arrived.
  """
  def complete(account_id, blob_id) do
    case Repo.get(Upload, blob_id) do
      %Upload{uploader_account_id: ^account_id, stored: true} ->
        :ok

      %Upload{uploader_account_id: ^account_id} = upload ->
        case Store.size(object(upload.id)) do
          {:ok, size} when size == upload.size ->
            mark_stored(upload)

          {:ok, _other} ->
            remove(from u in Upload, where: u.id == ^upload.id)
            {:error, {:invalid_argument, "size_bytes"}}

          _ ->
            {:error, :not_found}
        end

      _ ->
        {:error, :not_found}
    end
  end

  defp mark_stored(upload) do
    Repo.update_all(from(u in Upload, where: u.id == ^upload.id), set: [stored: true])
    :ok
  end

  @doc """
  Where the caller gets a blob's ciphertext: `{:redirect, url}` to the
  bucket, `{:bytes, bytes}` from a directory store, or `{:error,
  :not_found}` unless the caller can read the message naming it (its
  uploader alone while none does). A blob an edit dropped, or of a deleted
  message, is not found.
  """
  def fetch(account_id, blob_id) do
    with %Upload{stored: true, removed: false, deleting: false} = upload <-
           Repo.get(Upload, blob_id),
         :ok <- readable(account_id, upload),
         {:ok, where} <- location(upload) do
      where
    else
      _ -> {:error, :not_found}
    end
  end

  defp location(upload) do
    case Store.presign(object(upload.id), :get, @get_seconds) do
      {:ok, url} -> {:ok, {:redirect, url}}
      :instance -> with {:ok, bytes} <- Store.get(object(upload.id)), do: {:ok, {:bytes, bytes}}
      error -> error
    end
  end

  defp readable(account_id, %Upload{message_id: nil, uploader_account_id: account_id}), do: :ok
  defp readable(_account_id, %Upload{message_id: nil}), do: {:error, :not_found}

  defp readable(account_id, %Upload{} = upload) do
    with {:ok, access} <- Access.require(account_id, upload.channel_id, :read_history) do
      if Dms.personal_kind?(access.channel.kind) or
           Repo.exists?(
             from m in Message,
               where:
                 m.id == ^upload.message_id and m.channel_id == ^upload.channel_id and
                   is_nil(m.deleted_at)
           ),
         do: :ok,
         else: {:error, :not_found}
    end
  end

  @doc """
  Removes a blob at its uploader's word: one of a personal conversation
  (its sender deleted the message, specs/media.md 4.4), or one no message
  has named yet (taken back before sending). A server message's blobs go
  with the message. `:ok`, or `{:error, :not_found}`.
  """
  def delete(account_id, blob_id) do
    with %Upload{uploader_account_id: ^account_id, deleting: false} = upload <-
           Repo.get(Upload, blob_id),
         true <- upload.message_id == nil or personal?(upload.channel_id) do
      remove(from u in Upload, where: u.id == ^upload.id)
      :ok
    else
      _ -> {:error, :not_found}
    end
  end

  defp personal?(channel_id) do
    Repo.one(from c in Lumia.Events.Channel, where: c.id == ^channel_id, select: c.kind)
    |> Dms.personal_kind?()
  end

  # ---- Sending ----------------------------------------------------------------

  @doc """
  Checks a message's attachments before it is written (specs/media.md
  4.2): at most #{@max_attachments}, each well formed, each blob the
  sender's, stored, for this channel, named by no other message than
  `message_id` (a copy of a send that already committed names its own),
  of the role its kind needs and of the length it says. `{:ok,
  attachments}`, stripped of fields this instance does not know, or
  `{:error, {:invalid_argument, "attachments"}}`.
  """
  def check_attachments(_channel_id, _account_id, _message_id, []), do: {:ok, []}

  def check_attachments(_channel_id, _account_id, _message_id, attachments)
      when length(attachments) > @max_attachments,
      do: {:error, {:invalid_argument, "attachments"}}

  def check_attachments(channel_id, account_id, message_id, attachments) do
    if Enum.all?(attachments, &well_formed?/1) and
         distinct?(Enum.map(attachments, & &1.attachment_id)) and
         blobs_held?(channel_id, account_id, message_id, attachments),
       do: {:ok, Enum.map(attachments, &known/1)},
       else: {:error, {:invalid_argument, "attachments"}}
  end

  defp distinct?(list), do: list == Enum.uniq(list)

  # What a client added beyond the schema would be kept and delivered with
  # the message, outside every limit here.
  defp known(%V1.Attachment{} = a) do
    %{
      a
      | __unknown_fields__: [],
        main: known_ref(a.main),
        thumbnail: a.thumbnail && known_ref(a.thumbnail)
    }
  end

  defp known_ref(%V1.BlobRef{} = ref), do: %{ref | __unknown_fields__: []}

  # Every blob named is the sender's, stored, for this channel, unnamed,
  # of the role its place needs and of the length its reference says.
  defp blobs_held?(channel_id, account_id, message_id, attachments) do
    refs =
      Enum.flat_map(attachments, fn a ->
        [{a.main, main_role(a.kind)}] ++
          if(a.thumbnail, do: [{a.thumbnail, "thumbnail"}], else: [])
      end)

    ids = Enum.map(refs, fn {ref, _} -> Proto.from_uuid(ref.blob_id) end)
    held = held(channel_id, account_id, message_id, ids) |> Map.new(&{&1.id, &1})

    Enum.all?(ids) and distinct?(ids) and
      Enum.all?(Enum.zip(refs, ids), fn {{ref, role}, id} ->
        match?(%Upload{role: ^role}, held[id]) and held[id].size == ref.size_bytes
      end)
  end

  # The blobs among `ids` a message may name: the sender's, stored, for
  # this conversation, being removed by no one, and named by no message
  # but this one.
  defp held(channel_id, account_id, message_id, ids) do
    Repo.all(
      from u in Upload,
        where:
          u.id in ^Enum.reject(ids, &is_nil/1) and u.uploader_account_id == ^account_id and
            u.channel_id == ^channel_id and u.stored and not u.removed and not u.deleting and
            (is_nil(u.message_id) or u.message_id == ^message_id)
    )
  end

  defp main_role(:IMAGE), do: "display"
  defp main_role(_kind), do: "file"

  defp well_formed?(%V1.Attachment{} = a) do
    Proto.from_uuid(a.attachment_id) != nil and a.kind in [:IMAGE, :GIF, :VIDEO, :AUDIO, :FILE] and
      text_fits?(a) and blob_ref?(a.main) and thumbnail_fits?(a)
  end

  defp text_fits?(a) do
    a.name != "" and byte_size(a.name) <= @max_name_bytes and String.valid?(a.name) and
      String.length(a.alt) <= @max_alt_chars and
      byte_size(a.media_type) <= @max_media_type_bytes and
      byte_size(a.placeholder) <= @max_placeholder_bytes
  end

  defp thumbnail_fits?(%V1.Attachment{thumbnail: nil}), do: true

  defp thumbnail_fits?(%V1.Attachment{thumbnail: thumbnail, kind: kind}),
    do: kind in [:IMAGE, :GIF, :VIDEO] and blob_ref?(thumbnail)

  defp blob_ref?(%V1.BlobRef{key: <<_::256>>, nonce: <<_::192>>, hash: <<_::256>>}), do: true
  defp blob_ref?(_), do: false

  @doc "The ids of every blob `attachments` name."
  def blob_ids(attachments) do
    Enum.flat_map(attachments, fn a ->
      [a.main, a.thumbnail] |> Enum.reject(&is_nil/1) |> Enum.map(&Proto.from_uuid(&1.blob_id))
    end)
  end

  @doc """
  Checks the blobs a personal message says it uses (`SendEncrypted.blob_ids`):
  each the sender's, stored, for this conversation and named by no other
  message than `message_id`. `:ok` or `{:error, {:invalid_argument,
  "blob_ids"}}`.
  """
  def check_dm_blobs(_channel_id, _account_id, _message_id, []), do: :ok

  def check_dm_blobs(channel_id, account_id, message_id, blob_ids) do
    ids = Enum.map(blob_ids, &Proto.from_uuid/1)

    if length(ids) <= @max_dm_blobs and Enum.all?(ids) and distinct?(ids) and
         length(held(channel_id, account_id, message_id, ids)) == length(ids),
       do: :ok,
       else: {:error, {:invalid_argument, "blob_ids"}}
  end

  @doc "Marks blobs as named by `message_id`, so the sweep keeps them."
  def reference(_channel_id, _message_id, []), do: :ok

  def reference(channel_id, message_id, ids) do
    Repo.update_all(
      from(u in Upload, where: u.id in ^ids and u.channel_id == ^channel_id),
      set: [message_id: message_id]
    )

    :ok
  end

  @doc "Marks blobs an edit dropped: no longer served, and gone with their message."
  def mark_removed([]), do: :ok

  def mark_removed(ids) do
    Repo.update_all(from(u in Upload, where: u.id in ^ids), set: [removed: true])
    :ok
  end

  # ---- Removal ----------------------------------------------------------------

  @doc """
  Removes the blobs of messages purged from a channel (retention, or a
  deleted message's window passing): the rows now, the objects after the
  commit. `message_ids` is a query of message ids.
  """
  def release(channel_id, message_ids) do
    remove(
      from u in Upload,
        where:
          u.channel_id == ^channel_id and u.message_id in subquery(message_ids) and
            not u.deleting
    )
  end

  @doc """
  The hourly sweep: blobs no message named within #{@orphan_hours} hours,
  those of messages deleted more than #{@deleted_days} days ago (whose
  attachments the message row then lets go too), and those whose objects
  the store did not let go before. Returns how many it removed.
  """
  def sweep do
    now = DateTime.utc_now()
    orphaned_before = DateTime.add(now, -@orphan_hours, :hour)
    deleted_before = DateTime.add(now, -@deleted_days, :day)

    deleted_messages =
      from m in Message,
        where: not is_nil(m.deleted_at) and m.deleted_at < ^deleted_before,
        select: m.id

    # Each is checked again as it is marked, so one a message named in
    # the meantime stays.
    orphans =
      remove(
        from u in Upload,
          where:
            u.id in subquery(
              from u in Upload,
                where:
                  is_nil(u.message_id) and u.created_at < ^orphaned_before and not u.deleting,
                select: u.id,
                limit: 1_000
            ) and is_nil(u.message_id)
      )

    deleted =
      remove(
        from u in Upload,
          where:
            u.id in subquery(
              from u in Upload,
                where: u.message_id in subquery(deleted_messages) and not u.deleting,
                select: u.id,
                limit: 1_000
            )
      )

    Repo.update_all(
      from(m in Message,
        where:
          not is_nil(m.deleted_at) and m.deleted_at < ^deleted_before and
            fragment("? <> '{}'", m.attachments)
      ),
      set: [attachments: []]
    )

    retried =
      from(u in Upload, where: u.deleting, select: u.id, limit: 1_000)
      |> Repo.all()
      |> delete_objects()

    orphans + deleted + retried
  end

  # Marks the blobs `query` finds as being removed, which stops their
  # serving at once, and after the commit asks the store to let each
  # object go, removing the row of each it did. A row whose object stays
  # is asked for again by the next sweep. Returns how many were marked.
  defp remove(query) do
    {:ok, ids} =
      Tx.transact(fn ->
        {_, rows} =
          Repo.update_all(
            query |> where([u], not u.deleting) |> select([u], {u.id, u.server_id, u.size}),
            set: [deleting: true, removed: true]
          )

        # A blob being removed no longer counts against its server.
        rows
        |> Enum.reject(fn {_, server_id, _} -> is_nil(server_id) end)
        |> Enum.group_by(fn {_, server_id, _} -> server_id end, fn {_, _, size} -> size end)
        |> Enum.each(fn {server_id, sizes} ->
          Repo.update_all(from(s in Server, where: s.id == ^server_id),
            inc: [stored_bytes: -Enum.sum(sizes)]
          )
        end)

        ids = Enum.map(rows, &elem(&1, 0))
        Tx.after_commit(fn -> delete_objects(ids) end)
        {:ok, ids}
      end)

    length(ids)
  end

  defp delete_objects(ids) do
    gone = Enum.filter(ids, &(Store.delete(object(&1)) == :ok))
    Repo.delete_all(from u in Upload, where: u.id in ^gone)
    length(gone)
  end
end
