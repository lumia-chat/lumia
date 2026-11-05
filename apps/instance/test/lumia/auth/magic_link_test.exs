defmodule Lumia.Auth.MagicLinkTest do
  use Lumia.DataCase, async: true

  import Swoosh.TestAssertions

  alias Lumia.Accounts
  alias Lumia.Auth.MagicLink

  defp account_with_email(email) do
    {:ok, a} =
      Accounts.create_account(%{handle: "m#{System.unique_integer([:positive])}", email: email})

    a
  end

  defp token_from_mail(email) do
    assert_email_sent(fn mail ->
      assert {_, ^email} = hd(mail.to)

      send(
        self(),
        {:token,
         mail.text_body
         |> String.split("\n")
         |> Enum.find(&String.match?(&1, ~r/^[A-Za-z0-9_-]{32}$/))}
      )
    end)

    assert_receive {:token, token}
    token
  end

  test "a link signs the device in once, within its lifetime" do
    email = "one#{System.unique_integer([:positive])}@example.com"
    account = account_with_email(email)
    assert :ok = MagicLink.request(email)
    token = token_from_mail(email)
    device = Ecto.UUID.generate()

    assert {:ok, session, _tokens} = MagicLink.complete(token, device)
    assert session.account_id == account.id
    assert {:error, :unauthenticated} = MagicLink.complete(token, device)
  end

  test "an expired link is refused" do
    email = "late#{System.unique_integer([:positive])}@example.com"
    account_with_email(email)
    assert :ok = MagicLink.request(email)
    token = token_from_mail(email)

    Lumia.Repo.update_all(MagicLink.Link,
      set: [expires_at: DateTime.add(DateTime.utc_now(), -1, :second)]
    )

    assert {:error, :unauthenticated} = MagicLink.complete(token, Ecto.UUID.generate())
  end

  test "an address without an account gets the same answer and no mail" do
    assert :ok = MagicLink.request("nobody#{System.unique_integer([:positive])}@example.com")
    refute_email_sent()
    assert {:error, :unauthenticated} = MagicLink.complete("nope", Ecto.UUID.generate())
  end

  test "five an hour per address" do
    email = "many#{System.unique_integer([:positive])}@example.com"
    account_with_email(email)
    for _ <- 1..5, do: assert(:ok = MagicLink.request(email))
    assert {:error, :rate_limited} = MagicLink.request(email)
  end
end
