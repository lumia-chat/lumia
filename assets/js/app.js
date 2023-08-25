// If you want to use Phoenix channels, run `mix help phx.gen.channel`
// to get started and then uncomment the line below.
// import "./user_socket.js"

// You can include dependencies in two ways.
//
// The simplest option is to put them in assets/vendor and
// import them using relative paths:
//
//     import "../vendor/some-package.js"
//
// Alternatively, you can `npm install some-package --prefix assets` and import
// them using a path starting with the package name:
//
//     import "some-package"
//

// Include phoenix_html to handle method=PUT/DELETE in forms and buttons.
import "phoenix_html";
// Establish Phoenix Socket and LiveView configuration.
import {Socket} from "phoenix";
import {LiveSocket} from "phoenix_live_view";
import topbar from "../vendor/topbar";
import hljs from 'highlight.js';

function highlightAll(where = document) {
  where.querySelectorAll('pre code').forEach((block) => {
    const language = block.getAttribute("class");
    console.log(`highlighting ${language}`);
    const { value: value } = hljs.highlight(block.innerText, { language });
    block.innerHTML = value;
  });
}

let hooks = {};

hooks.EnterSubmit = {
  mounted() {
    this.el.addEventListener("keydown", (e) => {
      if (e.key == "Enter" && e.shiftKey == false) {
        this.pushEvent("add-message", {text: this.el.innerText});

        // On `Enter` it is still inserting a newline, even though we clear
        // out the `innerHTML` content. I don't want this.
        // This works for now.
        // However, THERE HAS GOT TO BE A BETTER WAY!
        setTimeout(() => { this.el.innerHTML = ""; }, 80);
      }
    });
  }
};

hooks.Highlight = {
  mounted() {
    highlightAll(this.el);
  }
}

let csrfToken = document.querySelector("meta[name='csrf-token']").getAttribute("content");
let liveSocket = new LiveSocket("/live", Socket, {
  params: {_csrf_token: csrfToken},
  hooks
});

// Show progress bar on live navigation and form submits. Only displays if still
// loading after 120 ms.
topbar.config({barColors: {0: "#29d"}, shadowColor: "rgba(0, 0, 0, .3)"});

let topBarScheduled = undefined;
window.addEventListener("phx:page-loading-start", () => {
  if(!topBarScheduled) {
    topBarScheduled = setTimeout(() => topbar.show(), 200);
  };
});
window.addEventListener("phx:page-loading-stop", () => {
  // Topbar crap.
  clearTimeout(topBarScheduled);
  topBarScheduled = undefined;
  topbar.hide();

  // Highlight when page is done loading.
  highlightAll();
});

// connect if there are any LiveViews on the page
liveSocket.connect();

// expose liveSocket on window for web console debug logs and latency simulation:
// >> liveSocket.enableDebug()
// >> liveSocket.enableLatencySim(1000)  // enabled for duration of browser session
// >> liveSocket.disableLatencySim()
window.liveSocket = liveSocket;

