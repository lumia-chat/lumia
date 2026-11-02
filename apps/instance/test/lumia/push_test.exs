defmodule Lumia.PushTest do
  @moduledoc "Content-free push (architecture 15.5): the decision, the delivery, and Web Push's crypto."

  use Lumia.DataCase, async: false
  use Oban.Testing, repo: Lumia.Repo

  import Lumia.Fixtures

  alias Lumia.Events
  alias Lumia.Push
  alias Lumia.Push.WebPush
  alias Lumia.Servers
  alias Lumia.V1
  alias Lumia.Workers

  setup do
    Application.put_env(:lumia, :push_test_pid, self())
    on_exit(fn -> Application.delete_env(:lumia, :push_test_pid) end)
    :ok
  end

  defp subscribe(account, device_id, kind \\ :WEB_PUSH) do
    :ok =
      Push.put(account.id, device_id, %V1.PushSubscription{
        kind: kind,
        endpoint: "https://push.example/#{device_id}",
        p256dh: Base.url_encode64(:crypto.strong_rand_bytes(65), padding: false),
        auth: Base.url_encode64(:crypto.strong_rand_bytes(16), padding: false)
      })
  end

  defp run_enqueued do
    for %Oban.Job{args: args} <- all_enqueued(worker: Workers.Push) do
      assert :ok = perform_job(Workers.Push, args)
    end
  end

  describe "the decision" do
    test "a personal DM message pushes every offline device of the other participants, never the sender" do
      %{channel: channel, a: a, b: b} = personal_dm()
      b_phone = Ecto.UUID.generate()
      b_laptop = Ecto.UUID.generate()
      a_phone = Ecto.UUID.generate()
      subscribe(b, b_phone)
      subscribe(b, b_laptop, :UNIFIED_PUSH)
      subscribe(a, a_phone)
      {:ok, _} = LumiaWeb.Presence.track(self(), "devices", b_laptop, %{})

      actor = %{account_id: a.id, device_id: a_phone}
      {:ok, _} = Events.append(channel.id, actor, send_encrypted(channel.id, :APPLICATION, 1))
      assert_enqueued(worker: Workers.Push, args: %{channel_id: channel.id, seq: 1})
      run_enqueued()

      assert_receive {:push, %{device_id: ^b_phone, kind: "web_push"}, %{"body" => "New message"}}
      refute_receive {:push, %{device_id: ^b_laptop}, _}
      refute_receive {:push, %{device_id: ^a_phone}, _}
    end

    test "a control message is never pushed" do
      %{channel: channel, a: a, b: b} = personal_dm()
      subscribe(b, Ecto.UUID.generate())
      actor = %{account_id: a.id, device_id: Ecto.UUID.generate()}
      {:ok, _} = Events.append(channel.id, actor, send_encrypted(channel.id, :CONTROL, 1))
      refute_enqueued(worker: Workers.Push)
    end

    test "a server message pushes the mentioned accounts and nobody else" do
      owner = account("owner")
      mentioned = account("mentioned")
      bystander = account("bystander")
      {:ok, server} = Servers.create_server(owner.id, "Acme")
      {:ok, channel} = Servers.create_channel(server.id, owner.id, "general")
      {:ok, _} = Servers.add_member(server.id, mentioned.id, owner.id)
      {:ok, _} = Servers.add_member(server.id, bystander.id, owner.id)
      phone = Ecto.UUID.generate()
      subscribe(mentioned, phone)
      subscribe(bystander, Ecto.UUID.generate())
      actor = %{account_id: owner.id, device_id: Ecto.UUID.generate()}

      {:ok, _} = Events.append(channel.id, actor, send_message(channel.id, "hello all"))
      refute_enqueued(worker: Workers.Push)

      command = send_message(channel.id, "hi @mentioned")
      {:send_message, send} = command.op

      mention = %V1.Mention{
        target: {:account_id, Lumia.Proto.uuid(mentioned.id)},
        start: 3,
        end: 13
      }

      command = %{command | op: {:send_message, %{send | mentions: [mention]}}}
      {:ok, _} = Events.append(channel.id, actor, command)
      run_enqueued()

      assert_receive {:push, %{device_id: ^phone}, _}
      refute_receive {:push, _, _}
    end
  end

  describe "subscriptions" do
    test "one per device and kind, replaced on put, removed on delete" do
      a = account("a")
      device = Ecto.UUID.generate()
      subscribe(a, device)
      subscribe(a, device)
      assert [%{endpoint: "https://push.example/" <> _}] = Push.for_accounts([a.id])

      assert {:error, :invalid_argument} =
               Push.put(a.id, device, %V1.PushSubscription{
                 kind: :WEB_PUSH,
                 endpoint: "http://plain"
               })

      :ok = Push.delete(device, "web_push")
      assert Push.for_accounts([a.id]) == []
    end
  end

  describe "Web Push" do
    # RFC 8291 appendix A: fixed keys and salt give the published ciphertext.
    test "encrypts the RFC 8291 test vector" do
      ua_public =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"

      auth = "BTBZMqHH6r4Tts7J_aSIgg"
      salt = Base.url_decode64!("DGv6ra1nlYgDCS1FRnbzlw", padding: false)

      as_private =
        Base.url_decode64!("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw", padding: false)

      as_public =
        Base.url_decode64!(
          "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8",
          padding: false
        )

      body =
        WebPush.encrypt("When I grow up, I want to be a watermelon", ua_public, auth,
          salt: salt,
          keypair: {as_public, as_private}
        )

      assert Base.url_encode64(body, padding: false) ==
               "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
    end

    test "the VAPID authorization is an ES256 token over the endpoint's origin, verifiable with the public key" do
      {public, private} = :crypto.generate_key(:ecdh, :prime256v1)

      header =
        WebPush.authorization(
          "https://push.example:8443/x/y",
          public,
          private,
          "mailto:ops@example"
        )

      assert "vapid t=" <> rest = header
      [jwt, "k=" <> k] = String.split(rest, ", ")
      assert Base.url_decode64!(k, padding: false) == public
      [h, c, sig] = String.split(jwt, ".")
      assert %{"alg" => "ES256"} = Jason.decode!(Base.url_decode64!(h, padding: false))

      assert %{"aud" => "https://push.example:8443", "sub" => "mailto:ops@example"} =
               Jason.decode!(Base.url_decode64!(c, padding: false))

      <<r::binary-size(32), s::binary-size(32)>> = Base.url_decode64!(sig, padding: false)
      der = der_sequence(der_integer(r) <> der_integer(s))
      assert :crypto.verify(:ecdsa, :sha256, h <> "." <> c, der, [public, :prime256v1])
    end
  end

  defp der_integer(<<first, _::binary>> = raw) do
    stripped = raw |> :binary.decode_unsigned() |> :binary.encode_unsigned()
    stripped = if first == 0 and byte_size(stripped) == 0, do: <<0>>, else: stripped
    <<msb, _::binary>> = stripped
    body = if msb >= 0x80, do: <<0>> <> stripped, else: stripped
    <<0x02, byte_size(body)>> <> body
  end

  defp der_sequence(content), do: <<0x30, byte_size(content)>> <> content
end
