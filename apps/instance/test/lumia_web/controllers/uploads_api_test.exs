defmodule LumiaWeb.UploadsApiTest do
  @moduledoc """
  Attachments (specs/media.md 4): a slot, the ciphertext sent to it, the
  word it arrived, a message that names it, and who may fetch it then;
  edits that drop one, deletes, a personal conversation's blobs, and the
  sweep. The test instance keeps blobs in a directory, so uploads go to
  the instance's own signed URL; the bucket's presigned one is checked by
  its shape.
  """

  use LumiaWeb.ConnCase, async: false

  import Ecto.Query
  import Lumia.Fixtures

  alias Lumia.Auth
  alias Lumia.Blobs.Store
  alias Lumia.Events
  alias Lumia.Events.Message
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.Servers
  alias Lumia.Uploads
  alias Lumia.Uploads.Upload
  alias Lumia.V1

  @proto "application/x-protobuf"

  setup do
    owner = account("owner")
    grace = account("grace")
    stranger = account("stranger")
    {:ok, server} = Servers.create_server(owner.id, "Acme")
    {:ok, general} = Servers.create_channel(server.id, owner.id, "general")
    {:ok, _} = Servers.add_member(server.id, grace.id, owner.id)
    {:ok, _} = Lumia.Channels.join(general.id, grace.id)
    %{owner: owner, grace: grace, stranger: stranger, general: general}
  end

  defp authed(conn, account) do
    {:ok, _session, tokens} = Auth.create_session(account.id, Ecto.UUID.generate())

    conn
    |> put_req_header("authorization", "Bearer " <> tokens.access_token)
    |> put_req_header("accept", @proto)
  end

  defp slot(account, channel_id, role, size) do
    req = %V1.CreateUploadRequest{
      channel_id: Proto.uuid(channel_id),
      role: role,
      size_bytes: size
    }

    build_conn()
    |> authed(account)
    |> put_req_header("content-type", @proto)
    |> post("/api/v1/uploads", V1.CreateUploadRequest.encode(req))
  end

  # A blob uploaded and completed: its id and a BlobRef for it.
  defp upload(
         account,
         channel_id,
         role \\ :BLOB_ROLE_FILE,
         bytes \\ :crypto.strong_rand_bytes(100)
       ) do
    conn = slot(account, channel_id, role, byte_size(bytes))
    assert conn.status == 201
    response = V1.CreateUploadResponse.decode(conn.resp_body)
    assert response.upload_headers["content-length"] == Integer.to_string(byte_size(bytes))

    put_conn =
      build_conn()
      |> put_req_header("content-type", "application/octet-stream")
      |> put(response.upload_url, bytes)

    assert put_conn.status == 204

    complete =
      build_conn()
      |> authed(account)
      |> post("/api/v1/uploads/#{Proto.from_uuid(response.blob_id)}/complete")

    assert complete.status == 204

    {Proto.from_uuid(response.blob_id),
     %V1.BlobRef{
       blob_id: response.blob_id,
       size_bytes: byte_size(bytes),
       key: :crypto.strong_rand_bytes(32),
       nonce: :crypto.strong_rand_bytes(24),
       hash: :crypto.strong_rand_bytes(32)
     }}
  end

  defp attachment(kind, main, thumbnail \\ nil) do
    %V1.Attachment{
      attachment_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      kind: kind,
      name: "photo.jpg",
      media_type: "image/jpeg",
      main: main,
      thumbnail: thumbnail
    }
  end

  defp send(channel, account, body, attachments, card_urls \\ []) do
    command = send_message(channel.id, body)
    {:send_message, op} = command.op
    op = %{op | attachments: attachments, card_urls: card_urls}
    result = Events.append(channel.id, actor(account), %{command | op: {:send_message, op}})
    {result, Proto.from_uuid(command.command_id)}
  end

  defp fetch(account, blob_id),
    do: build_conn() |> authed(account) |> get("/api/v1/blobs/#{blob_id}")

  test "a picture goes up, is sent, and reaches those who can read the message", ctx do
    {display_id, display} =
      upload(ctx.grace, ctx.general.id, :BLOB_ROLE_DISPLAY, "display bytes, sealed")

    {thumb_id, thumb} = upload(ctx.grace, ctx.general.id, :BLOB_ROLE_THUMBNAIL)

    # Before a message names it, only its uploader can fetch it.
    assert fetch(ctx.grace, display_id).status == 200
    assert fetch(ctx.owner, display_id).status == 404

    {{:ok, _}, message_id} =
      send(ctx.general, ctx.grace, "look", [attachment(:IMAGE, display, thumb)])

    conn = fetch(ctx.owner, display_id)
    assert conn.status == 200
    assert conn.resp_body == "display bytes, sealed"
    assert fetch(ctx.owner, thumb_id).status == 200
    assert fetch(ctx.stranger, display_id).status == 404

    # The message carries it, keys and all, to whoever reads it.
    [event] =
      Events.events_after(ctx.general.id, 0, 200)
      |> Enum.filter(&match?(%V1.Event{payload: {:message_created, _}}, &1))
      |> Enum.take(-1)

    {:message_created, %V1.MessageCreated{message: message}} = event.payload
    assert [%V1.Attachment{main: %V1.BlobRef{key: key}}] = message.attachments
    assert key == display.key
    assert Repo.get(Message, message_id).attachments != []

    assert [%V1.Attachment{}] =
             Proto.message(Repo.get(Message, message_id)).attachments
  end

  test "each role has its cap, and the instance says what they are", ctx do
    assert slot(ctx.grace, ctx.general.id, :BLOB_ROLE_DISPLAY, 2 * 1024 * 1024 + 1).status == 413
    assert slot(ctx.grace, ctx.general.id, :BLOB_ROLE_THUMBNAIL, 256 * 1024 + 1).status == 413
    assert slot(ctx.grace, ctx.general.id, :BLOB_ROLE_FILE, 10 * 1024 * 1024 + 1).status == 413
    assert slot(ctx.grace, ctx.general.id, :BLOB_ROLE_FILE, 10 * 1024 * 1024).status == 201
    assert slot(ctx.grace, ctx.general.id, :BLOB_ROLE_UNSPECIFIED, 100).status == 400
    assert slot(ctx.stranger, ctx.general.id, :BLOB_ROLE_FILE, 100).status == 404

    info = build_conn() |> put_req_header("accept", @proto) |> get("/api/v1/instance")
    limits = V1.InstanceInfo.decode(info.resp_body).upload_limits
    assert limits.image_max_bytes == 2 * 1024 * 1024
    assert limits.file_max_bytes == 10 * 1024 * 1024
    assert limits.thumbnail_max_bytes == 256 * 1024
  end

  test "a body of another length is refused and the slot stays empty", ctx do
    response =
      V1.CreateUploadResponse.decode(
        slot(ctx.grace, ctx.general.id, :BLOB_ROLE_FILE, 50).resp_body
      )

    blob_id = Proto.from_uuid(response.blob_id)

    raw = fn url, body ->
      build_conn() |> put_req_header("content-type", "application/octet-stream") |> put(url, body)
    end

    assert raw.(response.upload_url, String.duplicate("x", 51)).status == 400
    assert raw.(response.upload_url, String.duplicate("x", 49)).status == 400

    assert build_conn()
           |> authed(ctx.grace)
           |> post("/api/v1/uploads/#{blob_id}/complete")
           |> Map.get(:status) == 404

    assert raw.("/api/v1/blobs/upload/forged", "x").status == 404
  end

  test "a message may name only its sender's stored, unnamed blobs of the right role", ctx do
    {_, grace_file} = upload(ctx.grace, ctx.general.id, :BLOB_ROLE_FILE)
    {_, owner_file} = upload(ctx.owner, ctx.general.id, :BLOB_ROLE_FILE)
    {_, display} = upload(ctx.grace, ctx.general.id, :BLOB_ROLE_DISPLAY)

    assert {{:error, {:invalid_argument, "attachments"}}, _} =
             send(ctx.general, ctx.grace, "x", [attachment(:FILE, owner_file)])

    # An IMAGE's main blob is a display one; a file's is not.
    assert {{:error, {:invalid_argument, "attachments"}}, _} =
             send(ctx.general, ctx.grace, "x", [attachment(:IMAGE, grace_file)])

    assert {{:error, {:invalid_argument, "attachments"}}, _} =
             send(ctx.general, ctx.grace, "x", [attachment(:FILE, %{grace_file | size_bytes: 99})])

    assert {{:error, {:invalid_argument, "attachments"}}, _} =
             send(ctx.general, ctx.grace, "x", [attachment(:FILE, %{grace_file | key: "short"})])

    assert {{:ok, _}, _} = send(ctx.general, ctx.grace, "x", [attachment(:IMAGE, display)])

    # Named once, never again.
    assert {{:error, {:invalid_argument, "attachments"}}, _} =
             send(ctx.general, ctx.grace, "again", [attachment(:IMAGE, display)])
  end

  test "an edit drops an attachment, and a delete stops serving the rest", ctx do
    {kept_id, kept} = upload(ctx.grace, ctx.general.id)
    {dropped_id, dropped} = upload(ctx.grace, ctx.general.id)
    a = attachment(:FILE, kept)
    b = attachment(:FILE, dropped)
    {{:ok, _}, message_id} = send(ctx.general, ctx.grace, "two", [a, b])

    edit = %V1.Command{
      command_id: Proto.uuid(Ecto.UUID.generate(version: 7)),
      channel_id: Proto.uuid(ctx.general.id),
      op:
        {:edit_message,
         %V1.EditMessage{
           message_id: Proto.uuid(message_id),
           body: "one",
           remove_attachment_ids: [b.attachment_id]
         }}
    }

    assert {:ok, %{seq: edit_seq}} = Events.append(ctx.general.id, actor(ctx.grace), edit)

    [%V1.Event{payload: {:message_edited, edited}}] =
      Events.events_after(ctx.general.id, 0, 200) |> Enum.take(-1)

    assert Enum.map(edited.attachments, & &1.attachment_id) == [a.attachment_id]
    assert fetch(ctx.owner, kept_id).status == 200
    assert fetch(ctx.owner, dropped_id).status == 404

    assert {:ok, _} =
             Events.append(
               ctx.general.id,
               actor(ctx.grace),
               delete_message(ctx.general.id, message_id)
             )

    assert fetch(ctx.owner, kept_id).status == 404
    assert fetch(ctx.grace, kept_id).status == 404

    # The edit's payload, which held the message's attachments, is purged
    # with the original's.
    assert [%V1.Event{payload: {:redacted, _}}] =
             Events.events_after(ctx.general.id, edit_seq - 1, 1)

    # Past the window, the sweep removes both blobs, objects and rows.
    long_ago = DateTime.add(DateTime.utc_now(), -31, :day)

    Repo.update_all(from(m in Message, where: m.id == ^message_id),
      set: [deleted_at: long_ago]
    )

    assert Uploads.sweep() == 2
    refute Repo.get(Upload, kept_id)
    refute Store.exists?("att/" <> kept_id)
    assert Repo.get(Message, message_id).attachments == []
  end

  test "an empty file sealed, the tag alone, may be uploaded; less may not", ctx do
    assert slot(ctx.grace, ctx.general.id, :BLOB_ROLE_FILE, 16).status == 201
    assert slot(ctx.grace, ctx.general.id, :BLOB_ROLE_FILE, 15).status == 400
  end

  test "an attachment's placeholder is bounded, and fields the instance does not know are dropped",
       ctx do
    {_, ref} = upload(ctx.grace, ctx.general.id)
    big = %{attachment(:FILE, ref) | placeholder: :binary.copy("x", 65)}

    assert {{:error, {:invalid_argument, "attachments"}}, _} =
             send(ctx.general, ctx.grace, "big", [big])

    # Field 99, length-delimited, five bytes: a field no schema here has.
    padded =
      V1.Attachment.decode(
        V1.Attachment.encode(attachment(:FILE, ref)) <> <<0x9A, 0x06, 5, "junk!">>
      )

    assert padded.__unknown_fields__ != []
    {{:ok, _}, message_id} = send(ctx.general, ctx.grace, "padded", [padded])

    [kept] = Repo.get(Message, message_id).attachments
    assert V1.Attachment.decode(kept).__unknown_fields__ == []
  end

  test "a copy of a send with attachments that committed meets its message, not its blobs", ctx do
    {_, ref} = upload(ctx.grace, ctx.general.id)
    command = send_message(ctx.general.id, "once")
    {:send_message, op} = command.op
    command = %{command | op: {:send_message, %{op | attachments: [attachment(:FILE, ref)]}}}
    assert {:ok, ack} = Events.append(ctx.general.id, actor(ctx.grace), command)

    # The same command again answers with the first's ack.
    assert {:ok, ^ack} = Events.append(ctx.general.id, actor(ctx.grace), command)

    # Once its commands row is gone (purged), a late copy finds the message
    # there and is a conflict, as one without attachments is; before, the
    # blobs it names refused it as if they were another message's.
    Repo.delete_all(from c in Lumia.Events.Command, where: c.channel_id == ^ctx.general.id)
    assert {:error, :conflict} = Events.append(ctx.general.id, actor(ctx.grace), command)
  end

  test "a blob whose object the store kept is asked for again by the sweep", ctx do
    {blob_id, _} = upload(ctx.grace, ctx.general.id)
    Repo.update_all(from(u in Upload, where: u.id == ^blob_id), set: [deleting: true])
    assert fetch(ctx.grace, blob_id).status == 404

    assert Uploads.sweep() == 1
    refute Repo.get(Upload, blob_id)
    refute Store.exists?("att/" <> blob_id)
  end

  test "a blob nobody named for a day is swept, and one taken back goes at once", ctx do
    {orphan_id, _} = upload(ctx.grace, ctx.general.id)
    {taken_back_id, _} = upload(ctx.grace, ctx.general.id)

    assert build_conn()
           |> authed(ctx.owner)
           |> delete("/api/v1/blobs/#{taken_back_id}")
           |> Map.get(:status) == 404

    assert build_conn()
           |> authed(ctx.grace)
           |> delete("/api/v1/blobs/#{taken_back_id}")
           |> Map.get(:status) == 204

    refute Store.exists?("att/" <> taken_back_id)

    assert Uploads.sweep() == 0
    yesterday = DateTime.add(DateTime.utc_now(), -25, :hour)
    Repo.update_all(from(u in Upload, where: u.id == ^orphan_id), set: [created_at: yesterday])
    assert Uploads.sweep() == 1
    refute Store.exists?("att/" <> orphan_id)
  end

  test "a personal conversation's blobs: named by id, read by its members, deleted by their sender" do
    %{channel: dm, a: a, b: b} = personal_dm()
    outsider = account("outsider")
    {blob_id, _} = upload(a, dm.id)
    assert slot(outsider, dm.id, :BLOB_ROLE_FILE, 100).status == 404

    command = send_encrypted(dm.id, :APPLICATION, 0)
    {:send_encrypted, op} = command.op
    named = %{command | op: {:send_encrypted, %{op | blob_ids: [Proto.uuid(blob_id)]}}}
    assert {:ok, _} = Events.append(dm.id, actor(a), named)

    assert fetch(b, blob_id).status == 200
    assert fetch(outsider, blob_id).status == 404

    # Another's blob, or one named already, cannot be named.
    {b_blob, _} = upload(b, dm.id)
    again = send_encrypted(dm.id, :APPLICATION, 0)
    {:send_encrypted, op} = again.op

    for ids <- [[b_blob], [blob_id]] do
      command = %{again | op: {:send_encrypted, %{op | blob_ids: Enum.map(ids, &Proto.uuid/1)}}}
      assert {:error, {:invalid_argument, "blob_ids"}} = Events.append(dm.id, actor(a), command)
    end

    assert build_conn() |> authed(b) |> delete("/api/v1/blobs/#{blob_id}") |> Map.get(:status) ==
             404

    assert build_conn() |> authed(a) |> delete("/api/v1/blobs/#{blob_id}") |> Map.get(:status) ==
             204

    assert fetch(b, blob_id).status == 404
  end

  test "a bucket's upload URL is presigned for the exact length" do
    options = [
      bucket: "lumia",
      endpoint: "https://example.r2.cloudflarestorage.com",
      region: "auto",
      access_key_id: "AKID",
      secret_access_key: "secret"
    ]

    url = Store.S3.presign(options, "0190-blob", :put, 900, 1234)
    uri = URI.parse(url)
    query = URI.decode_query(uri.query)

    assert uri.path == "/lumia/0190-blob"
    assert query["X-Amz-Expires"] == "900"
    assert query["X-Amz-SignedHeaders"] == "content-length;host"
    assert byte_size(query["X-Amz-Signature"]) == 64
  end
end
