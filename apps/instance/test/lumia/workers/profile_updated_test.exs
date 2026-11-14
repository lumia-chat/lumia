defmodule Lumia.Workers.ProfileUpdatedTest do
  @moduledoc "A changed name or picture reaches everyone who can see the account (issue #15)."

  use Lumia.DataCase, async: false
  use Oban.Testing, repo: Lumia.Repo

  import Lumia.Fixtures

  alias Lumia.Accounts
  alias Lumia.Servers
  alias Lumia.V1
  alias Lumia.Workers.ProfileUpdated

  # The accounts a ProfileUpdated on `account`'s log names, oldest first.
  defp told(account) do
    for %V1.AccountEvent{payload: {:profile_updated, %V1.ProfileUpdated{account: a}}} <-
          Accounts.events_after(account.id, 0),
        do: a
  end

  defp run_fan_out do
    for %Oban.Job{args: args} <- all_enqueued(worker: ProfileUpdated) do
      assert :ok = perform_job(ProfileUpdated, args)
    end
  end

  setup do
    ada = account("ada")
    grace = account("grace")
    stranger = account("stranger")
    {:ok, server} = Servers.create_server(ada.id, "Acme")
    {:ok, _} = Servers.add_member(server.id, grace.id, ada.id)
    %{personal: personal} = %{personal: personal_dm(ada)}
    %{ada: ada, grace: grace, stranger: stranger, friend: personal.b}
  end

  test "a new name reaches the server's members and the conversation's other side, not strangers",
       ctx do
    {:ok, _} = Accounts.update_account(ctx.ada, %{display_name: "Ada L."})
    # The account's own log has it at once; everyone else's once the job runs.
    assert [%V1.Account{display_name: "Ada L."}] = told(ctx.ada)
    assert told(ctx.grace) == []
    run_fan_out()

    for who <- [ctx.grace, ctx.friend] do
      assert [%V1.Account{display_name: "Ada L.", profile: nil} = a] = told(who)
      assert a.account_id == Lumia.Proto.uuid(ctx.ada.id)
    end

    assert told(ctx.stranger) == []
    # Nobody is told twice, the account itself not again.
    assert length(told(ctx.ada)) == 1
  end

  test "changes made while the job waits are one job, carrying the latest", ctx do
    {:ok, ada} = Accounts.update_account(ctx.ada, %{display_name: "One"})
    {:ok, _} = Accounts.update_account(ada, %{display_name: "Two"})
    assert length(all_enqueued(worker: ProfileUpdated)) == 1
    run_fan_out()
    assert [%V1.Account{display_name: "Two"}] = told(ctx.grace)
  end

  test "others/1 is everyone who shares a server or a conversation, once each", ctx do
    {:ok, other} = Servers.create_server(ctx.ada.id, "Other")
    {:ok, _} = Servers.add_member(other.id, ctx.grace.id, ctx.ada.id)

    assert Enum.sort(ProfileUpdated.others(ctx.ada.id)) ==
             Enum.sort([ctx.grace.id, ctx.friend.id])

    assert ProfileUpdated.others(ctx.stranger.id) == []
  end
end
