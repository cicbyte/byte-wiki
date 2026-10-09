// 任何脚本错误都在页面顶部显示，避免静默白图
window.__errs = [];
window.addEventListener("error", function(e){
  window.__errs.push(String(e.message) + " @ " + (e.filename||"") + ":" + (e.lineno||0));
  var b = document.getElementById("errbanner");
  if (b) { b.style.display = "block"; b.textContent = "脚本错误: " + window.__errs.join(" | "); }
});
// DATA 由 callchain_data.js 提供
const palette = ["#5aa2ff","#54d08c","#f0b429","#ef6f6c","#b58cff","#4ec9d4","#f78fb3","#9ad64f","#ff9f6b","#6fa8ff","#c9a1ff","#5fd0a8","#f6d365","#7dd8e8"];
const mods = Object.keys(DATA.modules).sort();
const modColor = {}; mods.forEach((m,i)=>modColor[m]=palette[i%palette.length]);
const nodeById = {}; DATA.nodes.forEach(n=>nodeById[n.id]=n);
const outEdges = {}, inEdges = {};
DATA.edges.forEach(e=>{ (outEdges[e.src]=outEdges[e.src]||[]).push(e); (inEdges[e.dst]=inEdges[e.dst]||[]).push(e); });

// vis 数据集
const vnodes = DATA.nodes.map(n=>({
  id: n.id, label: n.qualname.length>34 ? n.qualname.slice(0,33)+"…" : n.qualname,
  title: n.module+"\n"+n.file+":"+n.lineno,
  color: { background: modColor[n.module], border: n.is_entry? "#ffcf7d" : modColor[n.module],
           highlight:{background:"#ffffff",border:"#5aa2ff"} },
  shape: n.is_entry? "diamond" : (n.routes.length? "box":"ellipse"),
  font: { color:"#dbe4f5", size:11 },
  borderWidth: n.is_entry?2:1,
}));
const vedges = DATA.edges.map(e=>({
  from:e.src, to:e.dst, arrows:"to", color:{color:"#3a4767", highlight:"#ffcf7d", hover:"#5aa2ff"},
  font:{size:9,color:"#8496b8"}, label: e.kind==="heuristic"?"~":"",
  dashes: e.kind==="heuristic",
}));
const container=document.getElementById("graph");
const data={ nodes:new vis.DataSet(vnodes), edges:new vis.DataSet(vedges) };
const options={
  layout:{ hierarchical:{enabled:true,direction:"UD",sortMethod:"directed",levelSeparation:110,nodeSpacing:90} },
  physics:{ enabled:false },
  interaction:{ hover:true, navigationButtons:true, keyboard:true, multiselect:true },
  edges:{ smooth:{type:"cubicBezier"} },
};
const network=new vis.Network(container,data,options);
// 崩溃修复：初次分层布局完成后固化坐标并关闭 hierarchical。
// 层级布局是动态的——之后每次 hidden 批量更新（筛选/聚焦）都会触发整图重排+重绘风暴，
// 大图直接拖死渲染进程。固化后节点增删只影响可见性，不再重排。
setTimeout(function(){
  try{
    const pos=network.getPositions();
    network.setOptions({layout:{hierarchical:{enabled:false}}});
    const upd=Object.keys(pos).map(function(id){return {id:id, x:pos[id].x, y:pos[id].y};});
    data.nodes.update(upd);
  }catch(e){ /* 固化失败则维持原行为 */ }
},600);
document.getElementById("stat").textContent =
  `函数 ${DATA.nodes.length} · 调用边 ${DATA.edges.length} · 模块 ${mods.length} · 入口 ${DATA.nodes.filter(n=>n.is_entry).length}`;

// 图例
const legend=document.getElementById("legend");
// 注意：顶层 const 不能叫 top/name/location 等 window 保留属性，经典脚本会直接 SyntaxError
const legendMods=mods.slice(0,12);
legendMods.forEach(m=>{ const s=document.createElement("span"); s.innerHTML=`<i class="sw" style="background:${modColor[m]}"></i>${m}`; legend.appendChild(s); });
if(mods.length>12){const s=document.createElement("span");s.textContent=`…共${mods.length}个`;legend.appendChild(s);}

// 工具栏
const modsel=document.getElementById("modfilter");
mods.forEach(m=>{const o=document.createElement("option");o.value=m;o.textContent=`${m} (${DATA.modules[m].funcs})`;modsel.appendChild(o);});
function applyFilter(){
  const q=document.getElementById("search").value.trim().toLowerCase();
  const mv=modsel.value, ev=document.getElementById("entryfilter").value;
  const update=[];
  DATA.nodes.forEach(n=>{
    let show=true;
    if(mv && n.module!==mv) show=false;
    if(ev==="entry" && !n.is_entry) show=false;
    if(ev==="hub" && n.callers<10) show=false;
    if(q){ const hay=(n.qualname+" "+n.module+" "+n.routes.join(" ")).toLowerCase(); if(!hay.includes(q)) show=false; }
    update.push({id:n.id, hidden:!show});
  });
  data.nodes.update(update);
}
var _fit=null; // 搜索防抖：避免每个键入字符都触发全图 hidden 批量更新
document.getElementById("search").addEventListener("input",function(){clearTimeout(_fit);_fit=setTimeout(applyFilter,250);});
modsel.addEventListener("change",applyFilter);
document.getElementById("entryfilter").addEventListener("change",applyFilter);
document.getElementById("btnreset").addEventListener("click",()=>{
  document.getElementById("search").value=""; modsel.value=""; document.getElementById("entryfilter").value="";
  applyFilter(); network.fit();
});

// 入口面板: 路由入口分组在前，自定义入口(--entry)在后；点击只保留该入口的可达执行流
function buildEntryPanel(){
  const list=document.getElementById("entrylist");
  const routes=DATA.nodes.filter(n=>n.is_entry && n.routes.length);
  const customs=DATA.nodes.filter(n=>n.is_entry && !n.routes.length);
  const mk=(n)=>{
    const d=document.createElement("div");
    d.className="ent"+(n.routes.length?"":" custom");
    d.dataset.kw=(n.qualname+" "+n.routes.join(" ")).toLowerCase();
    d.innerHTML=`<b>${n.qualname.split(".").pop()}</b><span>${n.routes[0]||n.module.split(".").pop()}</span>`;
    d.addEventListener("click",()=>focusFromEntry(n.id));
    list.appendChild(d);
  };
  if(routes.length){ const g=document.createElement("div"); g.className="grp"; g.textContent=`路由入口 (${routes.length})`; list.appendChild(g); routes.forEach(mk); }
  if(customs.length){ const g=document.createElement("div"); g.className="grp"; g.textContent=`自定义入口 (${customs.length})`; list.appendChild(g); customs.forEach(mk); }
}
function focusFromEntry(nid){
  const keep=new Set([nid]); let frontier=[nid];
  while(frontier.length){ const nf=[]; frontier.forEach(x=>(outEdges[x]||[]).forEach(e=>{ if(!keep.has(e.dst)){keep.add(e.dst);nf.push(e.dst);} })); frontier=nf; }
  DATA.nodes.forEach(n=>data.nodes.update({id:n.id, hidden:!keep.has(n.id)}));
  showDetail(nid); network.selectNodes([nid]); network.fit();
  const info=document.getElementById("entrysearch");
  info.placeholder=`已聚焦该入口执行流（${keep.size} 节点）· 重置视图恢复`;
}
document.getElementById("entrysearch").addEventListener("input",function(){
  const q=this.value.trim().toLowerCase();
  document.querySelectorAll("#entrylist .ent").forEach(el=>{
    el.style.display = !q || (el.dataset.kw||"").includes(q) ? "" : "none";
  });
});
buildEntryPanel();

// 详情面板
function escapeHtml(t){ return String(t).replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;"); }
function numberedSource(n){
  if(!n.source) return "(源码未嵌入)";
  return n.source.split("\n").map((l,i)=>String(n.lineno+i).padStart(5)+"  "+l).join("\n");
}
function showDetail(nid){
  const n=nodeById[nid]; if(!n) return;
  const side=document.getElementById("side");
  const callers=(inEdges[nid]||[]).map(e=>{
    const c=nodeById[e.src]; return `<div class="meta">← <a href="#" data-jump="${e.src}">${c?c.qualname:e.src}</a> <span style="color:#5b6b8c">${c?c.module:""} :${e.line}</span></div>`;
  }).join("") || `<div class="empty">无（入口/未被调用）</div>`;
  const callees=(outEdges[nid]||[]).map(e=>{
    const c=nodeById[e.dst]; return `<div class="meta">→ <a href="#" data-jump="${e.dst}">${c?c.qualname:e.dst}</a> <span style="color:#5b6b8c">${c?c.module:""} :${e.line}${e.kind==="heuristic"?" ~":""}</span></div>`;
  }).join("") || `<div class="empty">无（叶子函数）</div>`;
  side.innerHTML=`
    <div class="card">
      <h3>${n.qualname}</h3>
      <div class="meta">📦 ${n.module}</div>
      <div class="meta">📄 ${n.file}:${n.lineno}-${n.end_lineno}${n.async?" · async":""}</div>
      <div class="meta">入度 ${n.callers} · 出度 ${n.callees}</div>
      ${n.routes.map(r=>`<span class="tag route">🚪 ${r}</span>`).join("")}
      ${n.is_entry?'<span class="tag entry">入口</span>':""}
    </div>
    <div class="card"><h3>被谁调用</h3>${callers}</div>
    <div class="card"><h3>调用了谁</h3>${callees}</div>
    <div class="card"><h3>源码 <span style="color:#5b6b8c;font-weight:400;font-size:12px">${n.file}:${n.lineno}-${n.end_lineno}</span></h3><pre class="src">${escapeHtml(numberedSource(n))}</pre></div>`;
  side.querySelectorAll("a[data-jump]").forEach(a=>a.addEventListener("click",ev=>{
    ev.preventDefault(); const t=a.dataset.jump; network.selectNodes([t]); showDetail(t);
    network.focus(t,{scale:1.1});
  }));
}
let selected=null;
network.on("click",params=>{
  if(params.nodes.length){ selected=params.nodes[0]; showDetail(selected); }
});
network.on("doubleClick",params=>{
  if(params.nodes.length){ selected=params.nodes[0]; focusChain(selected); }
});

// 聚焦: 选中节点的全部祖先+后代
function focusChain(nid){
  const keep=new Set([nid]);
  let frontier=[nid];
  while(frontier.length){ const nf=[]; frontier.forEach(x=>(outEdges[x]||[]).forEach(e=>{ if(!keep.has(e.dst)){keep.add(e.dst);nf.push(e.dst);} })); frontier=nf; }
  frontier=[nid];
  while(frontier.length){ const nf=[]; frontier.forEach(x=>(inEdges[x]||[]).forEach(e=>{ if(!keep.has(e.src)){keep.add(e.src);nf.push(e.src);} })); frontier=nf; }
  const update=DATA.nodes.map(n=>({id:n.id,hidden:!keep.has(n.id)}));
  data.nodes.update(update); showDetail(nid); network.selectNodes([nid]); network.fit();
}
document.getElementById("btnfocus").addEventListener("click",()=>{ if(selected) focusChain(selected); });
