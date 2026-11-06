defmodule Lumia.ProtoJSON do
  @moduledoc """
  The API's JSON encoding: ProtoJSON with the rules of docs/protocol.md
  section 2 laid over it.

    1. A `bytes` field that is an id (its name ends in `_id` or `_ids`),
       `content_address` or `nonce` is a string a person can read: a
       canonical lowercase UUID when it holds sixteen bytes (account, channel,
       message, device and the other ids), lowercase hex otherwise (an
       identity id is a 32-byte hash, a content address a 32-byte digest, a
       nonce 24 bytes). Never base64.
    2. 64-bit integers are decimal strings (ProtoJSON already).
    3. Enums are their names (ProtoJSON already).
    4. Every other `bytes` field is base64 (ProtoJSON already).

  Decoding takes the same forms back, and still takes base64 in an id
  field, so a client written against stock ProtoJSON keeps working.
  """

  alias Lumia.Proto
  alias Protobuf.FieldProps

  @hex_names ~w(content_address nonce)

  @doc "Encodes a `Lumia.V1` struct as the protocol's JSON."
  def encode!(%_{} = struct), do: Jason.encode!(to_map!(struct))

  @doc "The JSON-ready map of a `Lumia.V1` struct (what `encode!/1` serializes)."
  def to_map!(%module{} = struct) do
    case Protobuf.JSON.to_encodable(struct) do
      {:ok, data} -> walk(data, module, :encode)
      {:error, error} -> raise error
    end
  end

  @doc "Decodes the protocol's JSON as `module`; raises on JSON or shape that is not it."
  def decode!(json, module) when is_atom(module) do
    data = json |> IO.iodata_to_binary() |> Jason.decode!()

    case Protobuf.JSON.from_decoded(walk(data, module, :decode), module) do
      {:ok, struct} -> struct
      {:error, error} -> raise error
    end
  end

  # ---- The walk ----------------------------------------------------------------------

  # Rewrites the id-shaped bytes fields of `data` (a ProtoJSON map of
  # `module`) and descends into its messages. A value that is not the map a
  # message would be (a well-known type's string form, say) is left as is.
  defp walk(data, module, direction) when is_map(data) do
    props = module.__message_props__()
    by_name = Map.new(props.field_props, fn {_, p} -> {p.name, p} end)
    by_json = Map.new(props.field_props, fn {_, p} -> {p.json_name, p} end)

    Map.new(data, fn {key, value} ->
      case Map.get(by_json, key) || Map.get(by_name, key) do
        nil -> {key, value}
        prop -> {key, field(value, prop, direction)}
      end
    end)
  end

  defp walk(data, _module, _direction), do: data

  defp field(value, %FieldProps{type: :bytes} = prop, direction) do
    cond do
      not id_field?(prop) -> value
      prop.repeated? and is_list(value) -> Enum.map(value, &id(&1, prop, direction))
      true -> id(value, prop, direction)
    end
  end

  defp field(value, %FieldProps{map?: true, type: entry}, direction) when is_map(value) do
    value_prop =
      entry.__message_props__().field_props |> Map.values() |> Enum.find(&(&1.name == "value"))

    Map.new(value, fn {k, v} -> {k, field(v, value_prop, direction)} end)
  end

  defp field(value, %FieldProps{embedded?: true, repeated?: true, type: module}, direction)
       when is_list(value) do
    Enum.map(value, &walk(&1, module, direction))
  end

  defp field(value, %FieldProps{embedded?: true, type: module}, direction),
    do: walk(value, module, direction)

  defp field(value, _prop, _direction), do: value

  defp id_field?(%FieldProps{name: name}) do
    name in @hex_names or String.ends_with?(name, "_id") or String.ends_with?(name, "_ids")
  end

  # Encoding: ProtoJSON's base64 to the readable form. Decoding: the
  # readable form (or base64, kept) to the base64 ProtoJSON reads.
  defp id(value, prop, :encode) when is_binary(value) do
    case Base.decode64(value) do
      {:ok, bytes} -> readable(bytes, prop)
      :error -> value
    end
  end

  defp id(value, _prop, :decode) when is_binary(value) do
    cond do
      uuid?(value) -> Base.encode64(Proto.uuid(value))
      hex?(value) -> Base.encode64(Base.decode16!(value, case: :mixed))
      true -> value
    end
  end

  defp id(value, _prop, _direction), do: value

  defp readable(<<_::128>> = bytes, %FieldProps{name: name}) when name not in @hex_names do
    Proto.from_uuid(bytes)
  end

  defp readable(bytes, _prop), do: Base.encode16(bytes, case: :lower)

  defp uuid?(<<_::288>> = s), do: match?({:ok, _}, Ecto.UUID.dump(s))
  defp uuid?(_), do: false

  defp hex?(s) when byte_size(s) > 0 and rem(byte_size(s), 2) == 0,
    do: String.match?(s, ~r/^[0-9a-fA-F]+$/)

  defp hex?(_), do: false
end
