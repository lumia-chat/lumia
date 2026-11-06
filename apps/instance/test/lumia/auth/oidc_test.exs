defmodule Lumia.Auth.OidcTest do
  use Lumia.DataCase, async: false

  alias Lumia.Accounts.Account
  alias Lumia.Auth.MagicLink
  alias Lumia.Auth.Oidc
  alias Lumia.FakeOidc
  alias Lumia.Repo

  setup do
    FakeOidc.start()
    previous = Application.get_env(:lumia, :oidc)

    Application.put_env(:lumia, :oidc,
      issuer: FakeOidc.issuer(),
      client_id: "lumia-test",
      client_secret: "secret",
      name: "Sign in with Fake"
    )

    on_exit(fn ->
      if previous,
        do: Application.put_env(:lumia, :oidc, previous),
        else: Application.delete_env(:lumia, :oidc)
    end)

    :ok
  end

  defp run_flow(claims) do
    return_to = LumiaWeb.Endpoint.url() <> "/sign-in"
    assert {:ok, url} = Oidc.start(return_to)
    %URI{query: query} = URI.parse(url)
    params = URI.decode_query(query)
    assert String.starts_with?(url, FakeOidc.issuer() <> "/authorize")
    assert params["code_challenge_method"] == "S256"
    flow = Repo.one!(Oidc.Flow)
    session = :erlang.binary_to_term(flow.session_params)
    FakeOidc.expect(session.nonce, claims)
    assert {:ok, landed} = Oidc.callback(%{"code" => "code-1", "state" => params["state"]})
    assert String.starts_with?(landed, return_to <> "?magic=")
    token = landed |> String.split("magic=") |> List.last()
    assert {:ok, session, _tokens} = MagicLink.complete(token, Ecto.UUID.generate())
    session.account_id
  end

  test "a first sign-in makes an account from the claims, a second finds it" do
    sub = "sub-#{System.unique_integer([:positive])}"

    claims = %{
      "sub" => sub,
      "email" => "#{sub}@example.com",
      "preferred_username" => "Pat.Doe",
      "name" => "Pat Doe"
    }

    account_id = run_flow(claims)
    account = Repo.get!(Account, account_id)
    assert account.handle == "patdoe"
    assert account.email == "#{sub}@example.com"
    assert account.display_name == "Pat Doe"

    assert run_flow(claims) == account_id
    assert Repo.aggregate(Oidc.Flow, :count) == 0
  end

  test "an email already on an account never matches it; the new account gets no email" do
    sub = "sub-#{System.unique_integer([:positive])}"

    {:ok, existing} =
      Lumia.Accounts.create_account(%{
        handle: "owner#{System.unique_integer([:positive])}",
        email: "#{sub}@example.com"
      })

    account_id = run_flow(%{"sub" => sub, "email" => "#{sub}@example.com"})
    refute account_id == existing.id
    assert Repo.get!(Account, account_id).email == nil
  end

  test "a second subject with the same username gets a handle of its own" do
    first =
      run_flow(%{
        "sub" => "a-#{System.unique_integer([:positive])}",
        "preferred_username" => "Same.Name"
      })

    second =
      run_flow(%{
        "sub" => "b-#{System.unique_integer([:positive])}",
        "preferred_username" => "Same.Name"
      })

    refute first == second
    assert String.starts_with?(Repo.get!(Account, second).handle, "samename")
  end

  test "a return outside the instance and its web origins is refused, and a stale state is sent back with an error" do
    assert {:error, :invalid_argument} = Oidc.start("https://evil.example/sign-in")
    # The apps' own scheme is a return: the sheet comes back to the app on it.
    assert {:ok, _} = Oidc.start("lumia://sign-in")
    assert {:error, nil} = Oidc.callback(%{"code" => "x", "state" => "unknown"})
  end

  test "the trust card names the provider" do
    assert Oidc.name() == "Sign in with Fake"
  end
end
