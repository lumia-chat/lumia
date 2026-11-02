defmodule LumiaWeb.UiControllerTest do
  use LumiaWeb.ConnCase, async: false

  alias LumiaWeb.UiController

  setup do
    dir = UiController.ui_dir()
    index = Path.join(dir, "index.html")
    had? = File.regular?(index)
    on_exit(fn -> unless had?, do: File.rm(index) end)
    %{index: index, dir: dir}
  end

  test "every unclaimed path renders the installed UI's index.html", %{
    conn: conn,
    index: index,
    dir: dir
  } do
    File.mkdir_p!(dir)
    File.write!(index, "<!doctype html><title>Lumia</title>")

    for path <- ["/", "/sign-in", "/app?place=home&dm=abc", "/anything/deep"] do
      conn = get(conn, path)
      assert conn.status == 200
      assert response_content_type(conn, :html) =~ "text/html"
      assert conn.resp_body =~ "<title>Lumia</title>"
      assert get_resp_header(conn, "cache-control") == ["no-cache"]
    end
  end

  test "an unknown api path is still a JSON not-found", %{conn: conn, index: index, dir: dir} do
    File.mkdir_p!(dir)
    File.write!(index, "<!doctype html>")

    conn = conn |> put_req_header("accept", "application/json") |> get("/api/v1/nope")
    assert conn.status == 404
    assert %{"code" => "NOT_FOUND"} = json_response(conn, 404)
  end

  test "without a build the instance says so", %{conn: conn, index: index} do
    File.rm(index)
    conn = get(conn, "/sign-in")
    assert conn.status == 404
    assert conn.resp_body =~ "no web UI installed"
  end
end
