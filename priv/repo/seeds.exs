# Script for populating the database. You can run it as:
#
#     mix run priv/repo/seeds.exs
#
# Inside the script, you can read and write to any of your
# repositories directly:
#
#     Lumia.Repo.insert!(%Lumia.SomeSchema{})
#
# We recommend using the bang functions (`insert!`, `update!`
# and so on) as they will fail if something goes wrong.

alias Lumia.Accounts
alias Lumia.Repo
alias Lumia.Server

Repo.transaction(fn ->
  {:ok, general_channel} =
    Server.create_channel(%{name: "general", description: "General chat", is_private: false})

  {:ok, random_channel} =
    Server.create_channel(%{name: "random", description: "Random chat", is_private: false})

  {:ok, private_channel} =
    Server.create_channel(%{name: "private", description: "Private chat", is_private: true})

  # TODO: seed avatar, and add add first name, last name, and display name to users?
  # https://randomuser.me/api/portraits/#{Enum.random(1..99)}.jpg

  {:ok, user} =
    Accounts.register_user(%{
      first_name: "John",
      last_name: "Tester",
      email: "user@example.com",
      password: "password123!"
    })

  {:ok, _user_channel} = Server.join_channel(user, general_channel)
  {:ok, _user_channel} = Server.join_channel(user, random_channel)
  {:ok, _user_channel} = Server.join_channel(user, private_channel)
end)
