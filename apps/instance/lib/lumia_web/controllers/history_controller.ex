defmodule LumiaWeb.HistoryController do
  @moduledoc """
  History backup endpoints (docs/protocol.md section 9, e2ee spec section
  9). Everything is keyed by the signed-in account's identity; an account
  without one gets `FAILED_PRECONDITION`.
  """

  use Phoenix.Controller, formats: []

  alias Lumia.Accounts.Account
  alias Lumia.History
  alias Lumia.Proto
  alias Lumia.Repo
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def put_chunk(conn, %{"chunk_id" => chunk_id}) do
    with {:ok, identity_id} <- identity(conn),
         {:ok, req} <- ProtoConn.decode(conn, V1.PutHistoryChunkRequest),
         channel_id when is_binary(channel_id) <- Proto.from_uuid(req.channel_id),
         :ok <-
           History.put_chunk(
             identity_id,
             conn.assigns.current_device_id,
             chunk_id,
             channel_id,
             req.ciphertext,
             req.nonce
           ) do
      ProtoConn.no_content(conn)
    else
      nil -> {:error, :invalid_argument}
      error -> error
    end
  end

  def get_chunk(conn, %{"chunk_id" => chunk_id}) do
    with {:ok, identity_id} <- identity(conn),
         {:ok, chunk} <- History.get_chunk(identity_id, chunk_id) do
      ProtoConn.render(conn, %V1.GetHistoryChunkResponse{
        ciphertext: chunk.ciphertext,
        nonce: chunk.nonce
      })
    end
  end

  def delete_chunk(conn, %{"chunk_id" => chunk_id}) do
    with {:ok, identity_id} <- identity(conn),
         :ok <- History.delete_chunk(identity_id, chunk_id) do
      ProtoConn.no_content(conn)
    end
  end

  def put_manifest(conn, _params) do
    with {:ok, identity_id} <- identity(conn),
         {:ok, req} <- ProtoConn.decode(conn, V1.PutHistoryManifestRequest),
         {:ok, version} <- manifest_version(req.manifest),
         :ok <-
           History.put_manifest(
             identity_id,
             conn.assigns.current_device_id,
             version,
             V1.SignedHistoryManifest.encode(req.manifest)
           ) do
      ProtoConn.no_content(conn)
    end
  end

  def list_manifests(conn, _params) do
    with {:ok, identity_id} <- identity(conn) do
      manifests =
        identity_id
        |> History.list_manifests()
        |> Enum.map(&V1.SignedHistoryManifest.decode(&1.manifest))

      ProtoConn.render(conn, %V1.ListHistoryManifestsResponse{manifests: manifests})
    end
  end

  def delete_all(conn, _params) do
    with {:ok, identity_id} <- identity(conn),
         :ok <- History.delete_all(identity_id) do
      ProtoConn.no_content(conn)
    end
  end

  def usage(conn, _params) do
    with {:ok, identity_id} <- identity(conn) do
      {used, quota} = History.usage(identity_id)
      ProtoConn.render(conn, %V1.HistoryUsageResponse{bytes_used: used, bytes_quota: quota})
    end
  end

  # The version lives in the signed body; the instance reads it as an
  # integer and verifies nothing else (clients verify signatures on restore).
  defp manifest_version(%V1.SignedHistoryManifest{body: body}) when is_binary(body) do
    {:ok, V1.HistoryManifestBody.decode(body).version}
  rescue
    _ -> {:error, :invalid_argument}
  end

  defp manifest_version(_), do: {:error, :invalid_argument}

  defp identity(conn) do
    case Repo.get(Account, conn.assigns.current_account_id) do
      %Account{identity_id: id} when is_binary(id) -> {:ok, id}
      _ -> {:error, :failed_precondition}
    end
  end
end
