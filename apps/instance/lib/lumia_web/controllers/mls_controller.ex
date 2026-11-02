defmodule LumiaWeb.MlsController do
  @moduledoc "The MLS delivery-service endpoints (docs/protocol.md section 9, e2ee spec section 11)."

  use Phoenix.Controller, formats: []

  alias Lumia.Mls
  alias Lumia.Proto
  alias Lumia.V1
  alias LumiaWeb.ProtoConn

  action_fallback LumiaWeb.FallbackController

  def publish_key_packages(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.PublishKeyPackagesRequest),
         {:ok, _count} <-
           Mls.publish_key_packages(
             conn.assigns.current_account_id,
             conn.assigns.current_device_id,
             req.key_packages,
             req.last_resort_key_package
           ) do
      ProtoConn.no_content(conn)
    end
  end

  def claim(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.ClaimKeyPackageRequest),
         target when is_binary(target) <- Proto.from_uuid(req.account_id),
         {:ok, claimed} <-
           Mls.claim(conn.assigns.current_account_id, target, Proto.from_uuid(req.device_id),
             exclude_device: conn.assigns.current_device_id
           ) do
      ProtoConn.render(conn, %V1.ClaimKeyPackageResponse{
        key_packages:
          Enum.map(claimed, fn c ->
            %V1.ClaimKeyPackageResponse.Claimed{
              device_id: Proto.uuid(c.device_id),
              key_package: c.key_package
            }
          end)
      })
    else
      nil -> {:error, :invalid_argument}
      error -> error
    end
  end

  def publish_group_info(conn, _params) do
    with {:ok, req} <- ProtoConn.decode(conn, V1.PublishGroupInfoRequest),
         channel_id when is_binary(channel_id) <- Proto.from_uuid(req.channel_id),
         :ok <-
           Mls.put_group_info(
             channel_id,
             conn.assigns.current_account_id,
             req.epoch,
             req.group_info
           ) do
      ProtoConn.no_content(conn)
    else
      nil -> {:error, :invalid_argument}
      error -> error
    end
  end

  def get_group_info(conn, %{"channel_id" => channel_id}) do
    with {:ok, found} <- Mls.get_group_info(channel_id, conn.assigns.current_account_id) do
      ProtoConn.render(conn, %V1.GetGroupInfoResponse{
        group_info: found.group_info,
        epoch: found.epoch
      })
    end
  end
end
