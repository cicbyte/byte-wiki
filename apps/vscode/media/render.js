/* Wiki 预览渲染（webview 内运行；markdown-it UMD + mermaid IIFE 均为本地 vendor）。
 * 协议：加载完成 postMessage{ready} → 宿主回 {doc, path, content, dark} → 渲染；
 * 主题切换宿主重推同消息。bw-file:// 与相对 .md 链接拦截回传宿主。 */
(function () {
  var vscode = acquireVsCodeApi();
  var app = document.getElementById("app");
  var state = { content: "", dark: false, path: "" };
  var md = window.markdownit({ html: false, linkify: false, typographer: false });

  function renderMermaid() {
    document.querySelectorAll("#app pre code.language-mermaid, #app pre code.lang-mermaid").forEach(function (block) {
      var div = document.createElement("div");
      div.className = "mermaid";
      div.textContent = block.textContent;
      block.parentElement.replaceWith(div);
    });
    try {
      window.mermaid.initialize({
        startOnLoad: false,
        theme: state.dark ? "dark" : "default",
        securityLevel: "strict",
      });
      if (window.mermaid.run) {
        window.mermaid.run({ querySelector: ".mermaid" }).catch(function () { /* 单图失败不阻塞 */ });
      } else {
        window.mermaid.init(undefined, document.querySelectorAll(".mermaid"));
      }
    } catch (e) { /* mermaid 异常保留代码块 */ }
  }

  function render() {
    app.innerHTML = md.render(state.content);
    renderMermaid();
  }

  app.addEventListener("click", function (ev) {
    var a = ev.target.closest("a");
    if (!a) return;
    var href = a.getAttribute("href") || "";
    if (!href || href.startsWith("#")) return;
    ev.preventDefault();
    if (href.startsWith("bw-file://")) {
      var rest = href.slice("bw-file://".length);
      var q = "";
      var qi = rest.indexOf("?");
      if (qi >= 0) { q = rest.slice(qi + 1); rest = rest.slice(0, qi); }
      var p = rest;
      try { p = decodeURIComponent(rest); } catch (e) { /* 保留原样 */ }
      var m = /(?:^|&)L=(\d+)(?:-(\d+))?/.exec(q);
      vscode.postMessage({
        type: "openFile",
        path: p,
        start: m ? parseInt(m[1], 10) : undefined,
        end: m ? (m[2] ? parseInt(m[2], 10) : parseInt(m[1], 10)) : undefined,
      });
      return;
    }
    if (/^[a-z][a-z0-9+.-]*:/i.test(href)) return; // 外部协议不管
    if (/\.md(\?|#|$)/i.test(href)) {
      var h = href.split("#")[0];
      try { h = decodeURIComponent(h); } catch (e) { /* 保留原样 */ }
      vscode.postMessage({ type: "openDoc", href: h });
    }
  });

  window.addEventListener("message", function (ev) {
    var m = ev.data;
    if (!m) return;
    if (m.type === "doc") {
      state = { content: m.content || "", dark: !!m.dark, path: m.path || "" };
      render();
    } else if (m.type === "theme") {
      state.dark = !!m.dark;
      render();
    }
  });

  vscode.postMessage({ type: "ready" });
})();
