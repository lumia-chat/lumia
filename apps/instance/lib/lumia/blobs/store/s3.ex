defmodule Lumia.Blobs.Store.S3 do
  @moduledoc """
  Blobs in an S3-compatible bucket, over `Req` with its SigV4 signing
  (no SDK). `options` come from the environment at boot (`config/runtime.exs`):
  `:bucket`, `:endpoint` (the service's URL, path-style: the bucket is the
  first path segment, which every S3-compatible service takes), `:region`,
  `:access_key_id` and `:secret_access_key`.

  S3 is a capability the operator enables (architecture 10): the one
  destination is the endpoint the operator named.
  """

  def put(options, name, bytes) do
    case request(options, :put, name, body: bytes) do
      {:ok, %Req.Response{status: status}} when status in 200..299 -> :ok
      other -> failed(other)
    end
  end

  def get(options, name) do
    case request(options, :get, name, decode_body: false) do
      {:ok, %Req.Response{status: 200, body: body}} -> {:ok, body}
      {:ok, %Req.Response{status: 404}} -> {:error, :not_found}
      other -> failed(other)
    end
  end

  def delete(options, name) do
    case request(options, :delete, name, []) do
      {:ok, %Req.Response{status: status}} when status in 200..299 or status == 404 -> :ok
      other -> failed(other)
    end
  end

  def exists?(options, name) do
    match?({:ok, %Req.Response{status: 200}}, request(options, :head, name, []))
  end

  defp request(options, method, name, extra) do
    [
      method: method,
      url: "#{String.trim_trailing(options[:endpoint], "/")}/#{options[:bucket]}/#{name}",
      aws_sigv4: [
        access_key_id: options[:access_key_id],
        secret_access_key: options[:secret_access_key],
        region: options[:region],
        service: :s3
      ],
      retry: :transient,
      max_retries: 2,
      receive_timeout: 30_000
    ]
    |> Keyword.merge(Keyword.get(options, :req_options, []))
    |> Keyword.merge(extra)
    |> Req.request()
  end

  defp failed({:ok, %Req.Response{status: status}}), do: {:error, {:s3, status}}
  defp failed({:error, exception}), do: {:error, {:s3, Exception.message(exception)}}
end
