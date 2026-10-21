/* 走读链大纲渲染（webview 内运行）。协议：ready → 宿主回 {chain} → 渲染；
 * 块点击 → openFunction；调用链接点击 → focusNode（宿主映射回链内块）。 */
(function () {
  var vscode = acquireVsCodeApi();
  var app = document.getElementById("app");

  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }

  function render(d) {
    var main = {};
    (d.main_path || []).forEach(function (id) { main[id] = true; });
    var h = '<div class="chain-head">' + esc(d.root) + " · " + d.entries.length + " 块" +
      (d.truncated ? "（已截断）" : "") + "</div>";
    h += '<div class="chain-list">';
    d.entries.forEach(function (e) {
      h +=
        '<div class="chain-item' + (main[e.id] ? " main" : "") + '" data-file="' + esc(e.node.file) +
        '" data-s="' + e.node.lineno + '" data-e="' + e.node.end_lineno + '">' +
        '<div class="line1"><span class="num">' + esc(e.num) + "</span>" +
        (main[e.id] ? '<span class="star" title="主路径">★</span>' : "") +
        '<span class="name">' + esc(e.node.qualname) + "</span>" +
        '<span class="loc">' + esc(e.node.file) + ":" + e.node.lineno + "-" + e.node.end_lineno + "</span></div>";
      if (e.calls && e.calls.length) {
        h += '<div class="calls">' + e.calls.map(function (c) {
          return '<span class="call" data-id="' + esc(c.target_id) + '"><span class="cnum">' +
            esc(c.target_num || "?") + "</span> " + esc(c.target_name) + ":" + c.line + "</span>";
        }).join("") + "</div>";
      }
      h += "</div>";
    });
    h += "</div>";
    app.innerHTML = h;
  }

  document.addEventListener("click", function (ev) {
    var call = ev.target.closest(".call");
    if (call) {
      vscode.postMessage({ type: "focusNode", id: call.dataset.id });
      return;
    }
    var item = ev.target.closest(".chain-item");
    if (item) {
      vscode.postMessage({
        type: "openFunction",
        file: item.dataset.file,
        start: parseInt(item.dataset.s, 10),
        end: parseInt(item.dataset.e, 10),
      });
    }
  });

  window.addEventListener("message", function (ev) {
    if (ev.data && ev.data.type === "chain") render(ev.data.chain);
  });

  vscode.postMessage({ type: "ready" });
})();
