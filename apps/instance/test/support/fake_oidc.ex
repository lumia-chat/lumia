defmodule Lumia.FakeOidc do
  @moduledoc """
  An OIDC provider for the tests: discovery, a token endpoint that signs
  an ID token with its own RSA key for the nonce the test sets, its JWKS
  and userinfo. Served by Bandit on a port of its own.
  """

  use Plug.Router

  plug Plug.Parsers, parsers: [:urlencoded, :json], json_decoder: Jason
  plug :match
  plug :dispatch

  @port 4931

  def issuer, do: "http://127.0.0.1:#{@port}"

  def start do
    unless Process.whereis(__MODULE__.State) do
      jwk = JOSE.JWK.generate_key({:rsa, 2048})

      {:ok, _} =
        Agent.start_link(fn -> %{jwk: jwk, nonce: nil, claims: %{}} end, name: __MODULE__.State)

      {:ok, _} = Bandit.start_link(plug: __MODULE__, port: @port, startup_log: false)
    end

    :ok
  end

  @doc "The claims the next ID token carries (besides the standard ones) and its nonce."
  def expect(nonce, claims),
    do: Agent.update(__MODULE__.State, &%{&1 | nonce: nonce, claims: claims})

  get "/.well-known/openid-configuration" do
    json(conn, %{
      issuer: issuer(),
      authorization_endpoint: issuer() <> "/authorize",
      token_endpoint: issuer() <> "/token",
      jwks_uri: issuer() <> "/jwks",
      userinfo_endpoint: issuer() <> "/userinfo",
      response_types_supported: ["code"],
      subject_types_supported: ["public"],
      id_token_signing_alg_values_supported: ["RS256"],
      token_endpoint_auth_methods_supported: ["client_secret_basic", "client_secret_post"],
      code_challenge_methods_supported: ["S256"]
    })
  end

  post "/token" do
    %{jwk: jwk, nonce: nonce, claims: claims} = Agent.get(__MODULE__.State, & &1)
    now = System.os_time(:second)

    payload =
      Map.merge(
        %{
          "iss" => issuer(),
          "sub" => claims["sub"] || "subject-1",
          "aud" => "lumia-test",
          "exp" => now + 300,
          "iat" => now,
          "nonce" => nonce
        },
        claims
      )

    {_, id_token} = jwk |> JOSE.JWT.sign(%{"alg" => "RS256"}, payload) |> JOSE.JWS.compact()

    json(conn, %{
      access_token: "access-1",
      token_type: "Bearer",
      id_token: id_token,
      expires_in: 300
    })
  end

  get "/jwks" do
    {_, public} = Agent.get(__MODULE__.State, & &1.jwk) |> JOSE.JWK.to_public_map()
    json(conn, %{keys: [public]})
  end

  get "/userinfo" do
    claims = Agent.get(__MODULE__.State, & &1.claims)
    json(conn, Map.put_new(claims, "sub", "subject-1"))
  end

  match _ do
    send_resp(conn, 404, "")
  end

  defp json(conn, body) do
    conn
    |> put_resp_content_type("application/json")
    |> send_resp(200, Jason.encode!(body))
  end
end
