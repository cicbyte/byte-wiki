/* 调用链图渲染（webview 内运行；vis-network standalone UMD 本地 vendor）。
 * 协议：ready → 宿主推 {graph, mode, nodes/edges | modules/moduleEdges, dark}；
 * 节点单击 → openNode（跳源码）；双击 → refocus（换 seed 重取子图）。 */
(function () {
  var vscode = acquireVsCodeApi();
  var netEl = document.getElementById("net");
  var modeEl = document.getElementById("mode");
  var network = null;
  var nodeMeta = {};

  function colors(dark) {
    return {
      node: dark ? "#3c3c3c" : "#e8e8e8",
      nodeBorder: dark ? "#5a5a5a" : "#bbbbbb",
      entry: dark ? "#4f46e5" : "#6366f1",
      entryBorder: dark ? "#818cf8" : "#4f46e5",
      edge: dark ? "#555555" : "#b0b0b0",
      font: dark ? "#d4d4d4" : "#333333",
      edgeLabel: dark ? "#999999" : "#777777",
      highlight: "#f59e0b",
    };
  }

  function shortName(q) {
    var parts = String(q).split("::");
    return parts[parts.length - 1];
  }

  function render(d) {
    var c = colors(d.dark);
    var nodes = [];
    var edges = [];

    if (d.mode === "modules") {
      (d.modules || []).forEach(function (m) {
        nodes.push({ id: m, label: shortName(m) || m, title: m, shape: "box", font: { color: c.font } });
      });
      (d.moduleEdges || []).forEach(function (e, i) {
        edges.push({
          id: "me" + i,
          from: e[0],
          to: e[1],
          value: e[2],
          title: e[0] + " → " + e[1] + " ×" + e[2],
          color: { color: c.edge, highlight: c.highlight },
          font: { size: 9, color: c.edgeLabel, strokeWidth: 0 },
          arrows: "to",
        });
      });
      modeEl.textContent = "模块全景（边宽 = 聚合调用量；双击模块可看其函数子图）";
    } else {
      (d.nodes || []).forEach(function (n) {
        var isSeed = n.id === d.seed;
        var isEntry = n.is_entry || isSeed;
        nodes.push({
          id: n.id,
          label: shortName(n.qualname),
          title: n.id + "\n" + n.file + (n.lineno ? ":" + n.lineno : "") + (isSeed ? "\n★ 中心" : ""),
          color: {
            background: isEntry ? c.entry : c.node,
            border: isSeed ? c.highlight : isEntry ? c.entryBorder : c.nodeBorder,
          },
          font: { color: isEntry ? "#ffffff" : c.font },
          shape: "dot",
          size: isSeed ? 14 : 9,
          borderWidth: isSeed ? 3 : 1,
        });
        nodeMeta[n.id] = n;
      });
      (d.edges || []).forEach(function (e, i) {
        edges.push({
          id: "e" + i,
          from: e.src,
          to: e.dst,
          title: e.src + " → " + e.dst + " @L" + e.line + (e.kind !== "local" ? " (" + e.kind + ")" : ""),
          color: { color: c.edge, highlight: c.highlight },
          arrows: "to",
          smooth: { type: "continuous" },
        });
      });
      modeEl.textContent = (d.truncated ? "子图（已达节点上限，已截断）" : "子图") +
        " · 单击节点跳源码 · 双击换中心";
    }

    var data = { nodes: new window.vis.DataSet(nodes), edges: new window.vis.DataSet(edges) };
    var opts = {
      physics: { enabled: true, solver: "barnesHut", stabilization: { iterations: 250 } },
      interaction: { hover: true, tooltipDelay: 120 },
      edges: { scaling: { min: 1, max: 6 } },
    };
    if (network) network.destroy();
    network = new window.vis.Network(netEl, data, opts);
    network.on("click", function (params) {
      var id = params.nodes && params.nodes[0];
      var meta = nodeMeta[id];
      if (meta) {
        vscode.postMessage({ type: "openNode", id: meta.id, file: meta.file, start: meta.lineno });
      } else if (id && d.mode === "modules") {
        // 模块全景里点模块：没有单一文件可跳，走子图聚焦
        vscode.postMessage({ type: "refocus", id: id });
      }
    });
    network.on("doubleClick", function (params) {
      var id = params.nodes && params.nodes[0];
      if (id) vscode.postMessage({ type: "refocus", id: id });
    });
  }

  window.addEventListener("message", function (ev) {
    if (ev.data && ev.data.type === "graph") render(ev.data);
  });

  vscode.postMessage({ type: "ready" });
})();
