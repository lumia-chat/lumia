defmodule LumiaWeb.Router do
  use LumiaWeb, :router

  # Public endpoints: the trust card and sign-in.
  pipeline :public do
    plug :accepts, ["json", "x-protobuf"]
  end

  # Everything else: bearer token verified against the session row.
  pipeline :api do
    plug :accepts, ["json", "x-protobuf"]
    plug LumiaWeb.Plugs.Auth
    plug LumiaWeb.Plugs.ValidateIds
  end

  scope "/api/v1", LumiaWeb do
    pipe_through :public

    get "/instance", InstanceController, :show
    post "/accounts", AuthController, :create_account
    post "/accounts/by-identity", IdentityController, :create_account
    post "/auth/sign-in/password", AuthController, :sign_in_password
    post "/auth/sign-in/magic-link", AuthController, :magic_link
    post "/auth/sign-in/magic-link/complete", AuthController, :magic_link_complete
    get "/auth/oidc/start", OidcController, :start
    get "/auth/oidc/callback", OidcController, :callback
    post "/auth/passkey/assert/start", PasskeyController, :assert_start
    post "/auth/passkey/assert/finish", PasskeyController, :assert_finish
    post "/auth/sign-in/identity", IdentityController, :sign_in
    post "/auth/challenge", IdentityController, :challenge
    get "/identity/rosters/by-root/:root", IdentityController, :get_roster_by_root
    post "/auth/refresh", AuthController, :refresh
    get "/identity/link-bundles/:code", IdentityController, :get_link_bundle
    get "/invites/:code", InviteController, :show
  end

  scope "/api/v1", LumiaWeb do
    pipe_through :api

    post "/auth/sign-out", AuthController, :sign_out
    post "/auth/totp/enroll", AuthController, :totp_enroll
    post "/auth/totp/confirm", AuthController, :totp_confirm
    post "/auth/passkey/register/start", PasskeyController, :register_start
    post "/auth/passkey/register/finish", PasskeyController, :register_finish
    get "/auth/passkeys", PasskeyController, :index
    delete "/auth/passkeys/:passkey_id", PasskeyController, :delete
    post "/auth/reauth", IdentityController, :reauth
    get "/identity/roster", IdentityController, :get_roster
    get "/accounts/:account_id/roster", IdentityController, :get_account_roster
    post "/identity/roster", IdentityController, :publish_roster
    post "/identity/devices/revoke", IdentityController, :revoke_device
    post "/identity/link", IdentityController, :link
    post "/identity/link-bundles", IdentityController, :create_link_bundle
    put "/identity/backup", IdentityController, :put_backup
    get "/identity/backup", IdentityController, :get_backup
    get "/sessions", AuthController, :list_sessions
    delete "/sessions/:session_id", AuthController, :revoke_session

    get "/account/snapshot", AccountController, :snapshot
    put "/account/profile", AccountController, :put_profile
    get "/account/events", AccountController, :events
    get "/accounts/:account_id", AccountController, :show

    get "/servers", ServerController, :index
    post "/servers", ServerController, :create
    get "/servers/:server_id", ServerController, :show
    get "/servers/:server_id/channels", ServerController, :channels
    get "/servers/:server_id/members", ServerController, :members
    patch "/servers/:server_id/members/:account_id", ServerController, :update_member
    delete "/servers/:server_id/members/:account_id", ServerController, :remove_member
    get "/servers/:server_id/roles", ServerController, :roles
    post "/servers/:server_id/roles", ServerController, :create_role
    put "/servers/:server_id/roles/:role_id", ServerController, :update_role
    delete "/servers/:server_id/roles/:role_id", ServerController, :delete_role

    get "/servers/:server_id/invites", InviteController, :index
    post "/invites", InviteController, :create
    post "/invites/:code/accept", InviteController, :accept
    delete "/invites/:code", InviteController, :delete

    post "/channels", ChannelController, :create
    put "/channels/:channel_id/overwrites", ServerController, :set_overwrite
    get "/channels/:channel_id/events", ChannelController, :events
    get "/channels/:channel_id/snapshot", ChannelController, :snapshot
    get "/channels/:channel_id/messages", ChannelController, :messages
    post "/channels/:channel_id/commands", ChannelController, :command

    put "/push/subscription", PushController, :put
    delete "/push/subscription", PushController, :delete

    post "/dms/personal", DmController, :open_personal
    post "/dm-requests", DmController, :send_request
    post "/dm-requests/:request_id/respond", DmController, :respond

    post "/mls/key-packages", MlsController, :publish_key_packages
    post "/mls/key-packages/claim", MlsController, :claim
    post "/mls/group-info", MlsController, :publish_group_info
    get "/mls/group-info/:channel_id", MlsController, :get_group_info

    put "/history/chunks/:chunk_id", HistoryController, :put_chunk
    get "/history/chunks/:chunk_id", HistoryController, :get_chunk
    delete "/history/chunks/:chunk_id", HistoryController, :delete_chunk
    put "/history/manifest", HistoryController, :put_manifest
    get "/history/manifests", HistoryController, :list_manifests
    delete "/history", HistoryController, :delete_all
    get "/history/usage", HistoryController, :usage
  end

  # Enable LiveDashboard and Swoosh mailbox preview in development
  if Application.compile_env(:lumia, :dev_routes) do
    # If you want to use the LiveDashboard in production, you should put
    # it behind authentication and allow only admins to access it.
    # If your application does not have an admins-only section yet,
    # you can use Plug.BasicAuth to set up some basic authentication
    # as long as you are also using SSL (which you should anyway).
    import Phoenix.LiveDashboard.Router

    scope "/dev" do
      pipe_through [:fetch_session, :protect_from_forgery]

      live_dashboard "/dashboard", metrics: LumiaWeb.Telemetry
      forward "/mailbox", Plug.Swoosh.MailboxPreview
    end
  end

  # The web UI, when installed (LumiaWeb.UiController): every path the API
  # and the dev routes do not claim renders its index.html. Declared last,
  # with no pipeline: the browser's accept header is not the API's.
  scope "/", LumiaWeb do
    get "/*path", UiController, :index
  end
end
