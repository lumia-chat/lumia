defmodule Lumia.Blobs.Store.S3Test do
  use ExUnit.Case, async: true

  alias Lumia.Blobs.Store.S3

  # A bucket that is a table: what the adapter sends is what a store sees.
  defp bucket(test_pid) do
    objects = :ets.new(:objects, [:public])

    plug = fn conn ->
      {:ok, body, conn} = Plug.Conn.read_body(conn)
      signed = Plug.Conn.get_req_header(conn, "authorization")
      send(test_pid, {:s3, conn.method, conn.request_path, signed, body})

      case {conn.method, :ets.lookup(objects, conn.request_path)} do
        {"PUT", _} ->
          :ets.insert(objects, {conn.request_path, body})
          Plug.Conn.send_resp(conn, 200, "")

        {"GET", []} ->
          Plug.Conn.send_resp(conn, 404, "<Error><Code>NoSuchKey</Code></Error>")

        {"GET", [{_, object}]} ->
          Plug.Conn.send_resp(conn, 200, object)

        {"DELETE", _} ->
          :ets.delete(objects, conn.request_path)
          Plug.Conn.send_resp(conn, 204, "")
      end
    end

    [
      bucket: "lumia-blobs",
      endpoint: "https://s3.example.test/",
      region: "auto",
      access_key_id: "AKIDEXAMPLE",
      secret_access_key: "secret",
      req_options: [plug: plug, retry: false]
    ]
  end

  test "puts, gets and deletes under the bucket's path, signed" do
    options = bucket(self())
    name = String.duplicate("ab", 32)

    assert :ok = S3.put(options, name, <<0, 1, 2, 255>>)
    assert_received {:s3, "PUT", "/lumia-blobs/" <> ^name, [authorization], <<0, 1, 2, 255>>}
    assert authorization =~ "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/"
    assert authorization =~ "/auto/s3/aws4_request"

    assert {:ok, <<0, 1, 2, 255>>} = S3.get(options, name)
    assert :ok = S3.delete(options, name)
    assert {:error, :not_found} = S3.get(options, name)
    assert :ok = S3.delete(options, name)
  end

  test "a bucket that refuses is an error, not a crash" do
    refuse = fn conn ->
      Plug.Conn.send_resp(conn, 403, "<Error><Code>AccessDenied</Code></Error>")
    end

    options = Keyword.put(bucket(self()), :req_options, plug: refuse, retry: false)
    assert {:error, {:s3, 403}} = S3.put(options, "aa", "x")
    assert {:error, {:s3, 403}} = S3.get(options, "aa")
    assert {:error, {:s3, 403}} = S3.delete(options, "aa")
  end
end
