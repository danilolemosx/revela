"use strict";
// Interface do Revela. Fala com o backend (app/src/lib.rs) por invoke e pelo evento "estado".

const T = window.__TAURI__;
const { invoke, convertFileSrc } = T.core;
const $ = (s) => document.querySelector(s);
const $$ = (s) => [...document.querySelectorAll(s)];

// botões de linguagem de câmera: o texto em português aparece no botão, o termo em inglês vai no fim do prompt.
// Dentro do mesmo grupo só vale um.
const CHIPS = [
  { id: "geral", pt: "PLANO GERAL", en: "wide shot", g: "quadro" },
  { id: "medio", pt: "PLANO MÉDIO", en: "medium shot", g: "quadro" },
  { id: "close", pt: "CLOSE", en: "close-up shot", g: "quadro" },
  { id: "detalhe", pt: "DETALHE", en: "extreme close-up detail shot", g: "quadro" },
  { id: "plongee", pt: "PLONGÉE", en: "high-angle shot", g: "angulo" },
  { id: "l50", pt: "LENTE 50mm", en: "50mm lens", g: "lente" },
  { id: "noite", pt: "NOITE", en: "at night", g: "hora" },
  { id: "magica", pt: "HORA MÁGICA", en: "golden hour light", g: "hora" },
  { id: "pelicula", pt: "PELÍCULA 35mm", en: "shot on 35mm film, natural film grain", g: "estilo" },
  { id: "contra", pt: "CONTRA-PLONGÉE", en: "low-angle shot", g: "angulo" },
  { id: "zenital", pt: "ZENITAL", en: "top-down overhead shot", g: "angulo" },
  { id: "drone", pt: "DRONE", en: "aerial drone shot", g: "angulo" },
  { id: "l24", pt: "LENTE 24mm", en: "24mm wide-angle lens", g: "lente" },
  { id: "l85", pt: "LENTE 85mm", en: "85mm lens, shallow depth of field", g: "lente" },
  { id: "contraluz", pt: "CONTRALUZ", en: "backlit", g: "luz" },
  { id: "azul", pt: "HORA AZUL", en: "blue hour", g: "hora" },
  { id: "mao", pt: "CÂMERA NA MÃO", en: "handheld camera", g: "camera" },
  { id: "celular", pt: "CELULAR", en: "shot on a smartphone camera", g: "estilo" },
  { id: "cctv", pt: "CCTV", en: "CCTV security camera footage", g: "estilo" },
  { id: "vhs", pt: "VHS", en: "VHS camcorder footage, analog video artifacts", g: "estilo" },
  { id: "telejornal", pt: "TELEJORNAL", en: "TV news broadcast footage", g: "estilo" },
];
const CHIPS_VISIVEIS = 9;
const TAMANHOS = { "169": [1344, 768], "916": [768, 1344], "11": [1024, 1024], "239": [1536, 640] };
// segundos por passo por megapixel, até o app medir neste Mac (iguais aos do motor)
const SEG_PADRAO = { "rapida-0": 11.5, "rapida-1": 25, "rapida-2": 38, "caprichada-0": 25.3, "caprichada-1": 55, "caprichada-2": 83.6 };
const PASSOS = { rapida: 4, caprichada: 20 };
const PIXELS = 1024 * 1024;
const EXTENSOES = /\.(png|jpe?g|webp)$/i;

const est = {
  app: { motor: "fechado", tradutor: "fechado", segPasso: {}, favoritas: [], faltando: [] },
  modo: "criar",
  fotos: [null, null],
  chips: new Set(),
  maisChips: false,
  trad: { en: "", chave: null, editado: false, rodando: null, promessa: null },
  modoTrad: "traduzir",
  tam: "169",
  rascunho: false,
  qualidade: "rapida",
  seed: novoSeed(),
  seedFixo: false,
  atual: null,
  fila: [],
  galeria: [],
  sel: null,
  verPrevia: false,
  filtro: "todos",
  primeira: true,
  ligandoDesde: null,
};

let relogioTimer = null;
const aoFim = [];
let tradTimer = null;
let avisoTimer = null;

// ---------- utilidades ----------

function novoSeed() {
  return Math.floor(Math.random() * 2 ** 48);
}
function mult16(v) {
  return Math.max(1, Math.round(v / 16)) * 16;
}
function tamanhoIgual(l, a) {
  const e = Math.sqrt(PIXELS / (l * a));
  return [mult16(l * e), mult16(a * e)];
}
function nFotos() {
  return est.modo === "editar" ? est.fotos.filter(Boolean).length : 0;
}
function tamanhoBase() {
  if (est.tam === "foto" && est.modo === "editar" && est.fotos[0]) return tamanhoIgual(est.fotos[0].l, est.fotos[0].a);
  return TAMANHOS[est.tam] || TAMANHOS["169"];
}
function tamanhoFinal() {
  const [l, a] = tamanhoBase();
  return est.rascunho ? [mult16(l / 2), mult16(a / 2)] : [l, a];
}
function segPorMp(q, n) {
  const k = `${q}-${Math.min(n, 2)}`;
  return (est.app.segPasso && est.app.segPasso[k]) || SEG_PADRAO[k];
}
function estimar(q, n, l, a) {
  return PASSOS[q] * segPorMp(q, n) * ((l * a) / PIXELS) + 6;
}
function fmtEstimativa(s) {
  if (s < 55) return `~${Math.max(5, Math.round(s / 5) * 5)} s`;
  if (s < 90) return "~1 min";
  return `~${Math.round(s / 60)} min`;
}
function relogio(s) {
  s = Math.max(0, Math.round(s));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
function hora(seg) {
  const d = new Date(seg * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}
function proporcao(l, a) {
  if (!l || !a) return "";
  const r = l / a;
  if (Math.abs(r - 1) < 0.03) return "1:1";
  if (Math.abs(r - 16 / 9) < 0.06) return "16:9";
  if (Math.abs(r - 9 / 16) < 0.03) return "9:16";
  if (Math.abs(r - 2.39) < 0.08) return "2.39";
  return `${l}×${a}`;
}
function url(caminho) {
  return convertFileSrc(caminho);
}
function esc(s) {
  return String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}
function aviso(msg, erro = false, ms = 4500) {
  const a = $("#aviso");
  a.textContent = msg;
  a.classList.toggle("erro", erro);
  a.classList.remove("oculto");
  clearTimeout(avisoTimer);
  avisoTimer = setTimeout(() => a.classList.add("oculto"), erro ? ms * 2 : ms);
}
function nomeArquivo(c) {
  return String(c).split("/").pop();
}
function guardar() {
  try {
    localStorage.setItem("revela", JSON.stringify({ tam: est.tam, qualidade: est.qualidade, modoTrad: est.modoTrad, rascunho: est.rascunho }));
  } catch (_) {}
}
function lembrar() {
  try {
    const p = JSON.parse(localStorage.getItem("revela") || "{}");
    if (p.tam && (TAMANHOS[p.tam] || p.tam === "foto")) est.tam = p.tam;
    if (PASSOS[p.qualidade]) est.qualidade = p.qualidade;
    if (p.modoTrad === "detalhar") est.modoTrad = "detalhar";
    est.rascunho = !!p.rascunho;
  } catch (_) {}
}

// ---------- tradução ----------

function chaveTrad() {
  return JSON.stringify([$("#pt").value.trim(), est.modoTrad, nFotos() > 0 ? nFotos() : 0]);
}
function situacaoTrad() {
  if (!$("#pt").value.trim() && !est.trad.en) return "vazio";
  if (est.trad.editado) return "editado";
  if (est.trad.rodando) return "traduzindo";
  if (est.trad.chave === chaveTrad()) return "em-dia";
  return "velho";
}
function agendarTraducao() {
  clearTimeout(tradTimer);
  renderIngles();
  tradTimer = setTimeout(() => traduzirAgora(false), 900);
}
function traduzirAgora(forcar) {
  const pedido = $("#pt").value.trim();
  if (!pedido) {
    if (!est.trad.editado) {
      est.trad.en = "";
      est.trad.chave = chaveTrad();
    }
    renderIngles();
    return Promise.resolve();
  }
  if (est.trad.editado && !forcar) return Promise.resolve();
  const chave = chaveTrad();
  if (!forcar && (est.trad.chave === chave || est.trad.rodando === chave)) return est.trad.promessa || Promise.resolve();
  if (est.app.tradutor !== "ligado") {
    renderIngles();
    return Promise.resolve();
  }
  est.trad.rodando = chave;
  renderIngles();
  const fotos = nFotos();
  const p = invoke("traduzir", { pedido, modo: est.modoTrad, fotos })
    .then((en) => {
      if (chaveTrad() === chave) {
        est.trad.en = en;
        est.trad.chave = chave;
        est.trad.editado = false;
      }
    })
    .catch((e) => aviso(`Tradutor: ${e}`, true))
    .finally(() => {
      if (est.trad.rodando === chave) est.trad.rodando = null;
      renderIngles();
    });
  est.trad.promessa = p;
  return p;
}
// o inglês que vai para a IA (traduz antes, se o pedido mudou)
async function garantirIngles() {
  const s = situacaoTrad();
  if (s === "editado" || s === "em-dia") return est.trad.en.trim();
  if (est.app.tradutor === "ligado") {
    if (s === "traduzindo") await est.trad.promessa;
    if (situacaoTrad() !== "em-dia") await traduzirAgora(true);
    if (situacaoTrad() === "em-dia") return est.trad.en.trim();
  }
  if (est.trad.en.trim()) return est.trad.en.trim();
  // sem tradutor: vai o texto como está
  aviso("O tradutor não está pronto: o pedido vai como está escrito.", true);
  return $("#pt").value.trim();
}

// ---------- render ----------

function renderModo() {
  $("#mCriar").classList.toggle("on", est.modo === "criar");
  $("#mEditar").classList.toggle("on", est.modo === "editar");
  document.body.classList.toggle("criar", est.modo === "criar");
  $("#blocoFotos").classList.toggle("oculto", est.modo !== "editar");
  const editar = est.modo === "editar";
  const n = { pedido: editar ? 2 : 1, tam: editar ? 3 : 2, qual: editar ? 4 : 3 };
  $$("[data-n]").forEach((e) => (e.textContent = n[e.dataset.n]));
}

function renderFotos() {
  $$(".ref").forEach((el) => {
    const i = +el.dataset.slot;
    const f = est.fotos[i];
    if (!f) {
      el.className = "ref vazia";
      el.innerHTML = i === 0 ? "<b>+</b>FOTO 1<span>a foto que vai ser editada</span>" : "<b>+</b>FOTO 2 · opcional<span>ex.: o estilo, a roupa, o lugar</span>";
      return;
    }
    el.className = "ref";
    el.innerHTML = `<img src="${url(f.mini || f.caminho)}" alt=""><span class="tag">FOTO ${i + 1}</span><button class="x" title="Tirar">✕</button>` +
      (f.deEtapa ? '<span class="de">DA ETAPA ANTERIOR</span>' : "") + `<span class="dim">${f.l}×${f.a}</span>`;
    el.title = f.caminho;
  });
}

function renderChips() {
  const lista = est.maisChips ? CHIPS : CHIPS.slice(0, CHIPS_VISIVEIS);
  $("#chips").innerHTML = lista.map((c) => `<button class="chip${est.chips.has(c.id) ? " on" : ""}" data-c="${c.id}" title="${esc(c.en)}">${c.pt}</button>`).join("") +
    `<button class="chip mais" data-c="+">${est.maisChips ? "− MENOS" : "+ MAIS"}</button>`;
}

function termosEn() {
  return CHIPS.filter((c) => est.chips.has(c.id)).map((c) => c.en);
}

function renderIngles() {
  const s = situacaoTrad();
  const caixa = $("#ingles");
  caixa.className = `ingles ${s}`;
  const en = $("#en");
  if (document.activeElement !== en) en.value = est.trad.en;
  const t = termosEn();
  $("#termos").innerHTML = t.length ? `<span>+ ${esc(t.join(", "))}</span>` : "";
  const ap = $("#aprov");
  const textos = { vazio: "", editado: "✎ EDITADO À MÃO", traduzindo: "TRADUZINDO…", "em-dia": "✓ EM DIA", velho: "↻ VAI TRADUZIR" };
  ap.className = `aprov ${s}`;
  ap.textContent = textos[s];
  if (s === "velho" && est.app.tradutor !== "ligado") ap.textContent = est.app.tradutor === "ligando" ? "ESPERANDO O TRADUTOR" : "SEM TRADUTOR";
  $$("#segTrad span").forEach((e) => e.classList.toggle("on", e.dataset.m === est.modoTrad));
  $("#btEditarEn").classList.toggle("on", !en.readOnly);
  const dica = { ligado: "tradutor local · pronto", ligando: "tradutor local · abrindo…", erro: "tradutor com erro", ausente: "sem tradutor: escreva em inglês", fechado: "tradutor local" };
  $("#tradDica").textContent = dica[est.app.tradutor] || "tradutor local";
  $("#tradDica").title = est.app.tradutorMsg || "Qwen3 4B no llama.cpp, só neste Mac";
}

function renderTamanhos() {
  const editar = est.modo === "editar";
  if (est.tam === "foto" && !editar) est.tam = "169";
  $$(".tam").forEach((b) => {
    b.classList.toggle("on", b.dataset.t === est.tam);
    if (b.dataset.t === "foto") b.disabled = !editar;
  });
  const f = est.fotos[0];
  $("#tamFoto").textContent = editar && f ? tamanhoIgual(f.l, f.a).join("×") : "igual à foto 1";
  $("#rascunho").classList.toggle("on", est.rascunho);
}

function renderQualidade() {
  const [l, a] = tamanhoFinal();
  const n = nFotos();
  $$(".q").forEach((b) => b.classList.toggle("on", b.dataset.q === est.qualidade));
  $("#estRapida").textContent = `4 passos · ${fmtEstimativa(estimar("rapida", n, l, a))}`;
  $("#estCaprichada").textContent = `20 passos · ${fmtEstimativa(estimar("caprichada", n, l, a))}`;
}

function renderSeed() {
  if (!$("#seed input")) $("#seed").textContent = est.seed;
  $("#sSortear").classList.toggle("on", !est.seedFixo);
  $("#sFixar").classList.toggle("on", est.seedFixo);
}

function renderBotoes() {
  const g = $("#gerar");
  if (est.atual) {
    g.classList.add("ocupado");
    const p = est.atual.passo;
    g.innerHTML = `GERANDO…<small>${p ? `passo ${p.valor} de ${p.max}` : esc(est.atual.etapa)}</small>`;
  } else {
    g.classList.remove("ocupado");
    g.innerHTML = "GERAR<small>⌘ + Enter</small>";
  }
}

function renderSelo() {
  const s = $("#selo");
  const a = est.app;
  s.className = `selo ${a.motor}`;
  let t = "PROCURANDO O MOTOR…";
  if (a.motor === "ligado") t = a.desktop ? `MOTOR LIGADO · COMFYUI DESKTOP :${a.porta}` : `MOTOR LIGADO · 127.0.0.1:${a.porta}`;
  else if (a.motor === "ligando") t = `LIGANDO O MOTOR… ${est.ligandoDesde ? relogio((Date.now() - est.ligandoDesde) / 1000) : ""}`;
  else if (a.motor === "erro") t = "MOTOR COM ERRO · TENTAR DE NOVO";
  else if (a.motor === "ausente") t = "COMFYUI NÃO ENCONTRADO";
  else if (a.motor === "fechado" && a.motorMsg) t = "MOTOR DESLIGADO · LIGAR";
  $("#seloTxt").textContent = t;
  s.title = a.motorMsg || (a.motor === "ligado" ? "ComfyUI em modo offline, só neste Mac" : "");
}

function itemSel() {
  return est.galeria.find((g) => g.caminho === est.sel) || null;
}

function cadeia(item) {
  // etapas: segue a foto 1 de cada imagem para trás (até 5)
  const lista = [{ caminho: item.caminho, item }];
  let atual = item;
  for (let i = 0; i < 5; i++) {
    const ref = atual && atual.ficha && atual.ficha.modo === "editar" && atual.ficha.referencias && atual.ficha.referencias[0];
    if (!ref) break;
    const anterior = est.galeria.find((g) => g.caminho === ref) || null;
    lista.unshift({ caminho: ref, item: anterior });
    atual = anterior;
  }
  return lista;
}

function renderVisor() {
  const img = $("#imgVisor");
  const vazio = $("#vazioVisor");
  const it = itemSel();
  const a = est.atual;
  $("#etapas").classList.add("oculto");
  $("#btAntes").classList.add("oculto");
  if (a && (est.verPrevia || !it)) {
    $("#canto").innerHTML = '<span class="pisca"></span>PRÉVIA AO VIVO · A IMAGEM VAI APARECENDO';
    $("#canto2").textContent = `${a.job.largura}×${a.job.altura}`;
    if (a.previa) {
      img.src = a.previa;
      img.classList.remove("oculto");
      img.classList.add("previa");
      vazio.classList.add("oculto");
    } else {
      img.classList.add("oculto");
      vazio.classList.remove("oculto");
      vazio.innerHTML = `<b>${esc(a.etapa.toUpperCase())}</b><span>A prévia aparece no primeiro passo.</span>`;
    }
    return;
  }
  img.classList.remove("previa");
  if (!it && est.app.motor === "ausente") {
    img.classList.add("oculto");
    vazio.classList.remove("oculto");
    vazio.innerHTML = "<b>NÃO ACHEI O COMFYUI</b><span>O Revela usa o ComfyUI Desktop instalado neste Mac como motor.<br>Instale o ComfyUI Desktop, abra uma vez, e reabra o Revela.</span>";
    $("#canto").textContent = "";
    $("#canto2").textContent = "";
    return;
  }
  if (!it) {
    img.classList.add("oculto");
    vazio.classList.remove("oculto");
    vazio.innerHTML = "<b>NADA AINDA</b><span>Escreva o pedido à esquerda e clique em GERAR.<br>Os resultados ficam na pasta output e aparecem à direita.</span>";
    $("#canto").textContent = "";
    $("#canto2").textContent = "";
    return;
  }
  vazio.classList.add("oculto");
  img.classList.remove("oculto");
  const src = url(it.caminho);
  if (img.dataset.src !== src) {
    img.src = src;
    img.dataset.src = src;
  }
  $("#canto").textContent = `${it.doRevela ? "REVELA" : "COMFYUI"} · ${hora(it.modificado)}`;
  $("#canto2").textContent = it.nome;
  const c = cadeia(it);
  if (c.length > 1) {
    const e = $("#etapas");
    e.innerHTML = c.map((x, i) => {
      const nome = i === 0 ? "ORIGINAL" : `ETAPA ${i}`;
      const mini = x.item && x.item.mini ? x.item.mini : x.caminho;
      return `${i ? '<span class="seta">→</span>' : ""}<button class="${x.caminho === it.caminho ? "atual" : ""}" data-c="${esc(x.caminho)}" title="${esc(x.caminho)}"><img src="${url(mini)}" alt="">${nome}</button>`;
    }).join("");
    e.classList.remove("oculto");
  }
  if (it.ficha && it.ficha.modo === "editar" && it.ficha.referencias && it.ficha.referencias[0]) $("#btAntes").classList.remove("oculto");
}

function renderInfo() {
  const info = $("#info");
  const a = est.atual;
  if (a && (est.verPrevia || !itemSel())) {
    // gerando: mostra o que está indo para a IA
    const j = a.job;
    info.classList.add("vazia");
    $("#dados").innerHTML =
      `<div class="dado"><span class="rot">Seed</span><b>${j.seed}</b></div>` +
      `<div class="dado"><span class="rot">Tamanho</span><b>${j.largura}×${j.altura}</b></div>` +
      `<div class="dado"><span class="rot">Qualidade</span><b>${j.qualidade === "rapida" ? "RÁPIDA" : "CAPRICHADA"}</b></div>` +
      `<div class="pedido" title="${esc(j.prompt)}">${esc(j.pedido ? `“${j.pedido}”` : j.prompt)}</div>`;
    return;
  }
  const it = itemSel();
  info.classList.toggle("vazia", !it);
  if (!it) {
    $("#dados").innerHTML = '<div class="pedido">O seed, o tamanho e o pedido de cada imagem aparecem aqui.</div>';
    return;
  }
  const f = it.ficha;
  const q = f ? (f.qualidade === "rapida" ? "RÁPIDA" : "CAPRICHADA") : "—";
  const texto = f ? (f.pedido ? `“${f.pedido}”` : f.prompt) : "Sem ficha (imagem de fora do Revela).";
  $("#dados").innerHTML =
    `<div class="dado"><span class="rot">Seed</span><b>${f ? f.seed : "—"}</b></div>` +
    `<div class="dado"><span class="rot">Tamanho</span><b>${it.largura}×${it.altura}</b></div>` +
    `<div class="dado"><span class="rot">Qualidade</span><b>${q}${f && f.rascunho ? " · RASC." : ""}</b></div>` +
    `<div class="pedido" title="${esc(f ? f.prompt : "")}">${esc(texto)}</div>`;
  $("#btFav").classList.toggle("on", !!it.favorita);
  $("#btVariar").disabled = !f;
  $("#btCarregar").disabled = !f;
}

function filtrados() {
  const hoje = new Date();
  hoje.setHours(0, 0, 0, 0);
  return est.galeria.filter((g) => est.filtro === "todos" || (est.filtro === "fav" && g.favorita) || (est.filtro === "hoje" && g.modificado * 1000 >= hoje.getTime()));
}

function renderGaleria() {
  $$("#filtro button").forEach((b) => b.classList.toggle("on", b.dataset.f === est.filtro));
  $("#nGal").textContent = est.galeria.length;
  let html = "";
  const a = est.atual;
  if (a) {
    const pct = progresso() * 100;
    html += `<div class="mini-img gerando${est.verPrevia ? " sel" : ""}" data-gerando="1">${a.previa ? `<img src="${a.previa}" alt="">` : ""}<span>GERANDO<br>${Math.round(pct)}%</span></div>`;
  }
  const lista = filtrados();
  html += lista.map((g) => {
    const sel = !est.verPrevia && g.caminho === est.sel ? " sel" : "";
    return `<div class="mini-img${sel}" data-c="${esc(g.caminho)}" title="${esc(g.nome)}"><img loading="lazy" src="${url(g.mini || g.caminho)}" alt="">` +
      `${g.favorita ? '<span class="fav">★</span>' : ""}<span class="leg">${hora(g.modificado)} · ${proporcao(g.largura, g.altura)}</span></div>`;
  }).join("");
  if (!lista.length && !a) html = `<div class="nada">${est.filtro === "todos" ? "A pasta output está vazia." : "Nenhuma imagem neste filtro."}</div>`;
  $("#grade").innerHTML = html;
}

function renderFila() {
  const l = $("#filaLista");
  const itens = [];
  if (est.atual) itens.push({ ativo: true, job: est.atual.job });
  est.fila.forEach((job) => itens.push({ job }));
  if (itens.length < 2) {
    l.classList.add("oculto");
    return;
  }
  l.classList.remove("oculto");
  l.innerHTML = itens.map((x, i) => {
    const j = x.job;
    const desc = `${x.ativo ? "gerando" : "na fila"} · ${proporcao(j.largura, j.altura)} · ${j.qualidade === "rapida" ? "rápida" : "caprichada"}`;
    return `<div class="it${x.ativo ? " ativo" : ""}"><b>${x.ativo ? "▶" : i + 1}</b><div>${esc(j.titulo)}<small>${desc}</small></div>` +
      (x.ativo ? "" : `<button data-tirar="${i - 1}" title="Tirar da fila">✕</button>`) + "</div>";
  }).join("");
}

// 0..1: preparando 0–8 %, passos 8–95 %, revelando/salvando 95–100 %
function progresso() {
  const a = est.atual;
  if (!a) return 0;
  if (a.fim) return 0.97;
  if (!a.passo) return Math.min(0.08, ((Date.now() - a.inicio) / 1000 / Math.max(10, a.estimativa)) * 0.08);
  const { valor, max } = a.passo;
  let dentro = 0;
  if (a.segPasso) dentro = Math.min(0.95, (Date.now() - a.passoEm) / 1000 / a.segPasso);
  return 0.08 + 0.87 * Math.min(1, (valor + dentro) / max);
}

function renderProgresso() {
  const a = est.atual;
  const p = $("#progresso");
  if (!a) {
    p.classList.add("oculto");
    return;
  }
  p.classList.remove("oculto");
  const decorrido = (Date.now() - a.inicio) / 1000;
  let tempo, etapa, sub;
  if (a.passo && a.faltam != null) {
    const faltam = Math.max(0, a.faltam - (Date.now() - a.faltamEm) / 1000);
    tempo = relogio(faltam);
    etapa = `${a.etapa} · passo ${a.passo.valor} de ${a.passo.max}`;
    sub = `faltam ~${relogio(faltam)} · ${Math.round(a.segPasso)} s por passo`;
  } else if (a.passo) {
    tempo = relogio(decorrido);
    etapa = `${a.etapa} · passo ${a.passo.valor} de ${a.passo.max}`;
    sub = "medindo a velocidade…";
  } else {
    tempo = relogio(decorrido);
    etapa = a.etapa;
    sub = `estimativa ${fmtEstimativa(a.estimativa)}` + (est.primeira ? " · a 1ª depois de ligar demora mais (lê os modelos do disco)" : "");
  }
  if (a.fim) {
    etapa = a.etapa;
    sub = "quase lá";
  }
  $("#pTempo").textContent = tempo;
  $("#pEtapa").textContent = etapa;
  $("#pSub").textContent = sub;
  $("#pBarra").style.width = `${(progresso() * 100).toFixed(1)}%`;
  const max = a.passo ? a.passo.max : 0;
  const marcas = $("#pMarcas");
  if (marcas.dataset.max !== String(max)) {
    marcas.dataset.max = String(max);
    marcas.innerHTML = max > 1 ? Array.from({ length: max - 1 }, (_, i) => `<i style="position:absolute;top:0;bottom:0;width:1px;background:rgba(52,49,45,.25);left:${(8 + (87 * (i + 1)) / max).toFixed(2)}%"></i>`).join("") : "";
  }
}

function renderTudo() {
  renderModo();
  renderFotos();
  renderChips();
  renderIngles();
  renderTamanhos();
  renderQualidade();
  renderSeed();
  renderBotoes();
  renderSelo();
  renderVisor();
  renderInfo();
  renderGaleria();
  renderFila();
  renderProgresso();
}

// ---------- ações ----------

function setModo(m) {
  if (est.modo === m) return;
  est.modo = m;
  if (m === "editar" && est.fotos[0] && !TAMANHOS[est.tam]) est.tam = "foto";
  renderTudo();
  agendarTraducao();
}

async function definirFoto(slot, caminho, deEtapa = false) {
  try {
    const [l, a] = await invoke("dimensoes", { caminho });
    const g = est.galeria.find((x) => x.caminho === caminho);
    est.fotos[slot] = { caminho, l, a, mini: g && g.mini, deEtapa };
    if (est.modo !== "editar") est.modo = "editar";
    if (slot === 0 && deEtapa) est.tam = "foto";
    renderTudo();
    agendarTraducao();
  } catch (e) {
    aviso(`Não consegui abrir a foto: ${e}`, true);
  }
}

function slotLivre() {
  return est.fotos[0] ? 1 : 0;
}

async function escolherFoto(slot) {
  try {
    const r = await T.dialog.open({ multiple: false, filters: [{ name: "Imagens", extensions: ["png", "jpg", "jpeg", "webp"] }] });
    if (r) definirFoto(slot, typeof r === "string" ? r : r.path);
  } catch (e) {
    aviso(String(e), true);
  }
}

async function montarTrabalho(base) {
  if (base) return base;
  if (est.modo === "editar" && !est.fotos[0]) {
    aviso("Arraste a foto 1 (a que vai ser editada) ou mude para CRIAR DO ZERO.", true);
    return null;
  }
  const pedido = $("#pt").value.trim();
  if (!pedido && !est.trad.en.trim()) {
    aviso("Escreva o que você quer.", true);
    $("#pt").focus();
    return null;
  }
  const prompt = await garantirIngles();
  if (!prompt) return null;
  if (!est.seedFixo) est.seed = novoSeed();
  renderSeed();
  const [largura, altura] = tamanhoFinal();
  const fotos = est.modo === "editar" ? est.fotos.filter(Boolean).map((f) => f.caminho) : [];
  return {
    pedido,
    prompt,
    termos: termosEn(),
    fotos,
    largura,
    altura,
    rascunho: est.rascunho,
    qualidade: est.qualidade,
    seed: est.seed,
    titulo: (pedido || prompt).slice(0, 60),
  };
}

function aoEvento(ev) {
  const a = est.atual;
  if (!a) return;
  switch (ev.tipo) {
    case "enviando":
      a.etapa = a.job.fotos.length ? "enviando as fotos" : "enviando o pedido";
      break;
    case "enfileirado":
      a.etapa = ev.naFrente > 0 ? `esperando ${ev.naFrente} na fila do ComfyUI` : "preparando";
      break;
    case "etapa":
      a.etapa = ev.nome;
      if (ev.nome === "revelando a imagem" || ev.nome === "salvando") a.fim = true;
      break;
    case "passo":
      a.passo = { valor: ev.valor, max: ev.max };
      a.passoEm = Date.now();
      if (ev.segPasso) a.segPasso = ev.segPasso;
      if (ev.faltamS != null) {
        a.faltam = ev.faltamS + 4; // + revelar e salvar
        a.faltamEm = Date.now();
      }
      break;
    case "previa":
      est.previasVistas = (est.previasVistas || 0) + 1;
      a.previa = `data:image/jpeg;base64,${ev.dados}`;
      if (est.verPrevia || !itemSel()) renderVisor();
      renderGaleria();
      return;
  }
  renderBotoes();
  renderProgresso();
  if (!a.previa) renderVisor();
}

async function executar(job) {
  const n = job.fotos.length;
  est.atual = { job, inicio: Date.now(), etapa: "preparando", passo: null, faltam: null, previa: null, estimativa: estimar(job.qualidade, n, job.largura, job.altura) };
  est.verPrevia = true;
  renderTudo();
  clearInterval(relogioTimer);
  relogioTimer = setInterval(() => {
    renderProgresso();
    const t = $("#grade .gerando span");
    if (t) t.innerHTML = `GERANDO<br>${Math.round(progresso() * 100)}%`;
  }, 500);
  const canal = new T.core.Channel();
  canal.onmessage = aoEvento;
  try {
    const fim = await invoke("gerar", { pedido: job, canal });
    if (fim.cancelado) aviso("Cancelado.");
    else if (fim.item) {
      est.galeria = [fim.item, ...est.galeria.filter((g) => g.caminho !== fim.item.caminho)];
      est.sel = fim.item.caminho;
      est.primeira = false;
      aviso(`Pronto em ${relogio(fim.tempoS)} · ${fim.item.nome}`);
    }
  } catch (e) {
    aviso(`Não deu certo: ${e}`, true);
  } finally {
    clearInterval(relogioTimer);
    est.atual = null;
    est.verPrevia = false;
    renderTudo();
    aoFim.splice(0).forEach((f) => f());
    const prox = est.fila.shift();
    if (prox) executar(prox);
  }
}

async function gerarAgora() {
  if (est.atual) return;
  if (est.app.motor !== "ligado") {
    aviso(est.app.motor === "ligando" ? "O motor ainda está ligando. Use + FILA para gerar assim que ele estiver pronto." : "O motor não está ligado.", true);
    return;
  }
  const job = await montarTrabalho();
  if (job) return executar(job);
}

async function porNaFila(base) {
  const job = await montarTrabalho(base);
  if (!job) return;
  if (est.atual || est.app.motor !== "ligado") {
    est.fila.push(job);
    renderFila();
    aviso(est.app.motor === "ligado" ? `Na fila (${est.fila.length}).` : "Na fila: começa quando o motor ligar.");
  } else executar(job);
}

function variar() {
  const it = itemSel();
  if (!it || !it.ficha) return;
  const f = it.ficha;
  const job = {
    pedido: f.pedido || "",
    prompt: f.prompt,
    termos: [],
    fotos: f.modo === "editar" ? f.referencias || [] : [],
    largura: f.largura || it.largura,
    altura: f.altura || it.altura,
    rascunho: !!f.rascunho,
    qualidade: f.qualidade,
    seed: novoSeed(),
    titulo: `variação · ${(f.pedido || f.prompt).slice(0, 48)}`,
  };
  if (est.atual) porNaFila(job);
  else if (est.app.motor === "ligado") executar(job);
  else porNaFila(job);
}

async function carregarPedido(it) {
  if (!it || !it.ficha) return;
  const f = it.ficha;
  est.modo = f.modo === "editar" ? "editar" : "criar";
  est.fotos = [null, null];
  $("#pt").value = f.pedido || "";
  est.chips.clear();
  est.qualidade = f.qualidade || "rapida";
  est.seed = f.seed;
  est.seedFixo = true;
  est.rascunho = !!f.rascunho;
  const l = f.largura || it.largura, a = f.altura || it.altura;
  const base = est.rascunho ? [l * 2, a * 2] : [l, a];
  const preset = Object.keys(TAMANHOS).find((k) => Math.abs(TAMANHOS[k][0] - base[0]) <= 16 && Math.abs(TAMANHOS[k][1] - base[1]) <= 16);
  est.tam = preset || (est.modo === "editar" ? "foto" : "169");
  const refs = f.modo === "editar" ? (f.referencias || []).slice(0, 2) : [];
  for (let i = 0; i < refs.length; i++) {
    try {
      const [rl, ra] = await invoke("dimensoes", { caminho: refs[i] });
      const g = est.galeria.find((x) => x.caminho === refs[i]);
      est.fotos[i] = { caminho: refs[i], l: rl, a: ra, mini: g && g.mini, deEtapa: !!g };
    } catch (_) {
      aviso(`A foto ${i + 1} desta imagem não está mais no disco.`, true);
    }
  }
  est.trad.en = f.prompt;
  est.trad.editado = !f.pedido;
  est.trad.chave = chaveTrad();
  renderTudo();
  aviso("Pedido carregado com o mesmo seed: clique em GERAR para refazer igual.");
}

async function carregarGaleria() {
  try {
    const atual = await invoke("galeria", { limite: 400 });
    // mantém as que acabaram de ser geradas, se a lista do disco ainda não tiver
    est.galeria = atual;
    if (!est.sel || !est.galeria.some((g) => g.caminho === est.sel)) est.sel = est.galeria.length ? est.galeria[0].caminho : null;
    renderGaleria();
    renderVisor();
    renderInfo();
  } catch (e) {
    if (est.app.motor !== "ausente") aviso(`Galeria: ${e}`, true);
  }
}

function abrirAjustes() {
  const a = est.app;
  const linha = (rot, caminho, botao = "ABRIR") =>
    `<div class="linha"><span class="rot">${rot}</span><code title="${esc(caminho)}">${esc(caminho || "—")}</code>${caminho ? `<button data-abrir="${esc(caminho)}">${botao}</button>` : "<span></span>"}</div>`;
  let html = linha("Resultados (output)", a.saida) + linha("Fotos (input)", a.entrada) + linha("Modelos", a.modelos) + linha("ComfyUI", a.comfyDir) + linha("Registro do motor", a.log, "VER");
  html += `<div class="linha"><span class="rot">Motor</span><code>${esc(a.motor === "ligado" ? `porta ${a.porta}${a.desktop ? " (ComfyUI Desktop)" : " (escondido, modo offline)"}` : a.motorMsg || a.motor)}</code><span></span></div>`;
  html += `<div class="linha"><span class="rot">Tradutor</span><code>${esc(a.tradutor === "ligado" ? "Qwen3 4B Instruct · 127.0.0.1:8190" : a.tradutorMsg || a.tradutor)}</code><span></span></div>`;
  if (a.faltando && a.faltando.length) html += `<div class="linha"><span class="rot" style="color:#ef6a61">Faltam modelos</span><code>${esc(a.faltando.join(", "))}</code><span></span></div>`;
  $("#ajustes").innerHTML = html;
  $("#modal").classList.remove("oculto");
}

// ---------- estado do backend ----------

function aoEstado(novo) {
  const antes = est.app;
  est.app = novo;
  if (novo.motor === "ligando" && antes.motor !== "ligando") est.ligandoDesde = Date.now();
  if (novo.motor === "ligado" && antes.motor !== "ligado") {
    est.primeira = true;
    carregarGaleria();
    if (!est.atual && est.fila.length) executar(est.fila.shift());
  }
  if (novo.tradutor === "ligado" && antes.tradutor !== "ligado") traduzirAgora(false);
  if (novo.faltando && novo.faltando.length && !(antes.faltando || []).length) aviso(`Faltam modelos: ${novo.faltando.join(", ")}`, true, 8000);
  renderSelo();
  renderIngles();
  renderQualidade();
  renderVisor();
  esperas.splice(0).forEach((e) => (e.cond() ? e.ok() : esperas.push(e)));
}

// promessas que esperam um estado do backend (sem temporizador: numa janela escondida o macOS congela os timers)
const esperas = [];
function quando(cond) {
  return new Promise((ok) => (cond() ? ok() : esperas.push({ cond, ok })));
}

// ---------- ligações ----------

$("#mCriar").onclick = () => setModo("criar");
$("#mEditar").onclick = () => setModo("editar");

$("#pt").addEventListener("input", () => {
  if (est.trad.editado && !$("#pt").value.trim()) est.trad.editado = false;
  agendarTraducao();
});

$("#chips").addEventListener("click", (e) => {
  const b = e.target.closest(".chip");
  if (!b) return;
  if (b.dataset.c === "+") {
    est.maisChips = !est.maisChips;
  } else {
    const c = CHIPS.find((x) => x.id === b.dataset.c);
    if (est.chips.has(c.id)) est.chips.delete(c.id);
    else {
      CHIPS.filter((x) => x.g === c.g).forEach((x) => est.chips.delete(x.id));
      est.chips.add(c.id);
    }
  }
  renderChips();
  renderIngles();
});

$("#segTrad").addEventListener("click", (e) => {
  const m = e.target.dataset && e.target.dataset.m;
  if (!m || m === est.modoTrad) return;
  est.modoTrad = m;
  est.trad.editado = false;
  guardar();
  renderIngles();
  traduzirAgora(false);
});

$("#btEditarEn").onclick = () => {
  const en = $("#en");
  en.readOnly = !en.readOnly;
  if (!en.readOnly) en.focus();
  renderIngles();
};
$("#en").addEventListener("input", () => {
  est.trad.en = $("#en").value;
  est.trad.editado = true;
  renderIngles();
});
$("#en").addEventListener("blur", () => {
  $("#en").readOnly = true;
  renderIngles();
});
$("#btRetraduzir").onclick = () => {
  est.trad.editado = false;
  traduzirAgora(true);
};

$("#tamanhos").addEventListener("click", (e) => {
  const b = e.target.closest(".tam");
  if (!b || b.disabled) return;
  est.tam = b.dataset.t;
  guardar();
  renderTamanhos();
  renderQualidade();
});
$("#rascunho").onclick = () => {
  est.rascunho = !est.rascunho;
  guardar();
  renderTamanhos();
  renderQualidade();
};
$$(".q").forEach((b) => (b.onclick = () => {
  est.qualidade = b.dataset.q;
  guardar();
  renderQualidade();
}));

$("#sSortear").onclick = () => {
  est.seedFixo = false;
  est.seed = novoSeed();
  renderSeed();
};
$("#sFixar").onclick = () => {
  est.seedFixo = true;
  renderSeed();
};
$("#seed").onclick = () => {
  if ($("#seed input")) return;
  const b = $("#seed");
  b.innerHTML = `<input value="${est.seed}" inputmode="numeric">`;
  const i = b.querySelector("input");
  i.focus();
  i.select();
  const fim = (ok) => {
    const v = parseInt(i.value.replace(/\D/g, ""), 10);
    if (ok && Number.isSafeInteger(v)) {
      est.seed = v;
      est.seedFixo = true;
    }
    b.textContent = est.seed;
    renderSeed();
  };
  i.addEventListener("keydown", (e) => {
    if (e.key === "Enter") fim(true);
    if (e.key === "Escape") fim(false);
    e.stopPropagation();
  });
  i.addEventListener("blur", () => fim(true));
};

$("#gerar").onclick = gerarAgora;
$("#btFila").onclick = () => porNaFila();
$("#btCancelar").onclick = () => {
  invoke("cancelar");
  $("#pEtapa").textContent = "CANCELANDO…";
};

$(".refs").addEventListener("click", (e) => {
  const el = e.target.closest(".ref");
  if (!el) return;
  const slot = +el.dataset.slot;
  if (e.target.closest(".x")) {
    est.fotos[slot] = null;
    if (slot === 0 && est.fotos[1]) {
      est.fotos = [est.fotos[1], null];
    }
    renderTudo();
    agendarTraducao();
    return;
  }
  escolherFoto(slot);
});

$("#grade").addEventListener("click", (e) => {
  const el = e.target.closest(".mini-img");
  if (!el) return;
  if (el.dataset.gerando) {
    est.verPrevia = true;
  } else {
    est.sel = el.dataset.c;
    est.verPrevia = false;
  }
  renderVisor();
  renderInfo();
  renderGaleria();
});
$("#grade").addEventListener("dblclick", (e) => {
  const el = e.target.closest(".mini-img[data-c]");
  if (el) carregarPedido(est.galeria.find((g) => g.caminho === el.dataset.c));
});
$("#filtro").addEventListener("click", (e) => {
  const f = e.target.dataset && e.target.dataset.f;
  if (!f) return;
  est.filtro = f;
  renderGaleria();
});
$("#filaLista").addEventListener("click", (e) => {
  const b = e.target.closest("[data-tirar]");
  if (!b) return;
  est.fila.splice(+b.dataset.tirar, 1);
  renderFila();
});
$("#etapas").addEventListener("click", (e) => {
  const b = e.target.closest("button[data-c]");
  if (!b) return;
  if (est.galeria.some((g) => g.caminho === b.dataset.c)) {
    est.sel = b.dataset.c;
    renderVisor();
    renderInfo();
    renderGaleria();
  } else invoke("mostrar", { caminho: b.dataset.c, revelar: true }).catch((err) => aviso(String(err), true));
});

// antes/depois: segurando o botão, o visor mostra a foto 1
const antes = $("#btAntes");
const mostrarAntes = (sim) => {
  const it = itemSel();
  if (!it || !it.ficha || !it.ficha.referencias) return;
  $("#imgVisor").src = sim ? url(it.ficha.referencias[0]) : url(it.caminho);
  $("#imgVisor").dataset.src = sim ? "" : url(it.caminho);
  antes.textContent = sim ? "⇆ DEPOIS" : "⇆ ANTES";
};
antes.addEventListener("mousedown", () => mostrarAntes(true));
antes.addEventListener("mouseup", () => mostrarAntes(false));
antes.addEventListener("mouseleave", () => mostrarAntes(false));

$("#btVariar").onclick = variar;
$("#btUsar").onclick = () => {
  const it = itemSel();
  if (!it) return;
  est.fotos[1] = null;
  definirFoto(0, it.caminho, true);
};
$("#btCarregar").onclick = () => carregarPedido(itemSel());
$("#btFav").onclick = async () => {
  const it = itemSel();
  if (!it) return;
  try {
    await invoke("favoritar", { caminho: it.caminho, sim: !it.favorita });
    it.favorita = !it.favorita;
    renderInfo();
    renderGaleria();
  } catch (e) {
    aviso(String(e), true);
  }
};
$("#btAvid").onclick = (e) => {
  e.stopPropagation();
  $("#menuAvid").classList.toggle("oculto");
};
$("#menuAvid").addEventListener("click", async (e) => {
  const b = e.target.closest("button");
  const it = itemSel();
  if (!b || !it) return;
  $("#menuAvid").classList.add("oculto");
  try {
    const p = await invoke("para_avid", { caminho: it.caminho, preencher: b.dataset.p === "1" });
    aviso(`1920×1080 salva em output/Para o Avid: ${nomeArquivo(p)}`);
    invoke("mostrar", { caminho: p, revelar: true });
  } catch (err) {
    aviso(String(err), true);
  }
});
document.addEventListener("click", (e) => {
  if (!e.target.closest(".avid")) $("#menuAvid").classList.add("oculto");
});

$("#selo").onclick = () => {
  const m = est.app.motor;
  if (m === "erro" || m === "fechado") invoke("ligar_motor");
  else if (m === "ausente") abrirAjustes();
};
$("#engr").onclick = abrirAjustes;
$("#abrirOutput").onclick = () => est.app.saida && invoke("mostrar", { caminho: est.app.saida, revelar: false });
$("#fecharModal").onclick = () => $("#modal").classList.add("oculto");
$("#modal").addEventListener("click", (e) => {
  if (e.target.id === "modal") $("#modal").classList.add("oculto");
  const b = e.target.closest("[data-abrir]");
  if (b) invoke("mostrar", { caminho: b.dataset.abrir, revelar: false }).catch((err) => aviso(String(err), true));
});

document.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
    e.preventDefault();
    gerarAgora();
  }
  if (e.key === "Escape") {
    $("#modal").classList.add("oculto");
    $("#menuAvid").classList.add("oculto");
  }
});

// colar um print (⌘V): vira a foto 1 (ou a 2, se a 1 já existe)
document.addEventListener("paste", async (e) => {
  const itens = [...((e.clipboardData && e.clipboardData.items) || [])];
  const img = itens.find((i) => i.type.startsWith("image/"));
  if (!img) return;
  e.preventDefault();
  try {
    const buf = await img.getAsFile().arrayBuffer();
    const caminho = await invoke("colar", new Uint8Array(buf));
    definirFoto(slotLivre(), caminho);
  } catch (err) {
    aviso(`Não consegui colar: ${err}`, true);
  }
});

// arrastar fotos do Finder
T.webview.getCurrentWebview().onDragDropEvent((ev) => {
  const p = ev.payload;
  if (p.type === "enter" || p.type === "over") {
    document.body.classList.add("arrastando");
    if (est.modo !== "editar") setModo("editar");
    const x = p.position.x / window.devicePixelRatio, y = p.position.y / window.devicePixelRatio;
    const alvo = document.elementFromPoint(x, y);
    $$(".ref").forEach((r) => r.classList.toggle("alvo", !!alvo && r.contains(alvo)));
    return;
  }
  document.body.classList.remove("arrastando");
  $$(".ref").forEach((r) => r.classList.remove("alvo"));
  if (p.type !== "drop") return;
  const imagens = (p.paths || []).filter((c) => EXTENSOES.test(c));
  if (!imagens.length) {
    if ((p.paths || []).length) aviso("Arraste imagens (PNG, JPG ou WebP).", true);
    return;
  }
  const x = p.position.x / window.devicePixelRatio, y = p.position.y / window.devicePixelRatio;
  const alvo = document.elementFromPoint(x, y);
  const ref = alvo && alvo.closest(".ref");
  if (imagens.length >= 2) {
    definirFoto(0, imagens[0]).then(() => definirFoto(1, imagens[1]));
  } else definirFoto(ref ? +ref.dataset.slot : slotLivre(), imagens[0]);
});

// a galeria se atualiza quando a janela volta ao primeiro plano (o ComfyUI Desktop pode ter gerado algo)
window.addEventListener("focus", () => {
  if (!est.atual) carregarGaleria();
});
setInterval(() => est.app.motor === "ligando" && renderSelo(), 1000);

// ---------- autoteste (REVELA_AUTOTESTE=1, janela escondida) ----------

async function autoteste() {
  // tudo por promessas e eventos do app (nada de setTimeout): com a janela escondida, os timers congelam
  const log = (m) => invoke("registrar", { linha: `${new Date().toISOString().slice(11, 19)} ${m}` });
  const carregou = (src) => new Promise((r) => { const i = new Image(); i.onload = () => r(`${i.naturalWidth}×${i.naturalHeight}`); i.onerror = () => r("ERRO"); i.src = src; });
  const gerarEsperar = async (nome) => {
    est.previasVistas = 0;
    const antes = est.galeria.length ? est.galeria[0].caminho : "";
    const t0 = Date.now();
    await gerarAgora();
    const it = itemSel();
    if (!it || it.caminho === antes) throw `${nome}: não apareceu imagem nova (${$("#aviso").textContent})`;
    await log(`${nome}: ${it.nome} ${it.largura}×${it.altura} em ${((Date.now() - t0) / 1000).toFixed(0)} s · prévias: ${est.previasVistas} · seed ${it.ficha && it.ficha.seed} · ficha: ${it.doRevela ? "ok" : "FALTOU"} · miniatura ${await carregou(url(it.mini))} · visor ${await carregou(url(it.caminho))}`);
    return it;
  };
  const traduzir = async (texto) => {
    $("#pt").value = texto;
    clearTimeout(tradTimer);
    await traduzirAgora(true);
    if (situacaoTrad() !== "em-dia") throw `tradução não ficou em dia (${situacaoTrad()})`;
  };
  try {
    await log("início");
    const t0 = Date.now();
    await quando(() => est.app.motor === "ligado" || est.app.motor === "erro" || est.app.motor === "ausente");
    if (est.app.motor !== "ligado") throw `motor: ${est.app.motor} ${est.app.motorMsg}`;
    await log(`motor ligado (${((Date.now() - t0) / 1000).toFixed(0)} s) porta ${est.app.porta} desktop=${est.app.desktop}`);
    await quando(() => est.app.tradutor !== "ligando" && est.app.tradutor !== "fechado");
    if (est.app.tradutor !== "ligado") throw `tradutor: ${est.app.tradutor} ${est.app.tradutorMsg}`;
    await carregarGaleria();
    await log(`tradutor pronto · galeria com ${est.galeria.length} imagens`);
    if (est.galeria[0]) await log(`miniatura da galeria: ${await carregou(url(est.galeria[0].mini))}`);

    setModo("criar");
    est.tam = "169";
    est.rascunho = true;
    est.qualidade = "rapida";
    est.chips = new Set(["geral"]);
    await traduzir("um farol numa ilha de pedra durante uma tempestade, ondas enormes batendo nas rochas");
    await log(`tradução: ${est.trad.en} + [${termosEn().join(", ")}]`);
    const a = await gerarEsperar("criar (rascunho 16:9)");

    const avid = await invoke("para_avid", { caminho: a.caminho, preencher: true });
    await log(`para o Avid: ${nomeArquivo(avid)} ${(await invoke("dimensoes", { caminho: avid })).join("×")}`);

    est.fotos[1] = null;
    await definirFoto(0, a.caminho, true);
    if (est.modo !== "editar" || !est.fotos[0]) throw "usar como referência não mudou para Editar";
    est.chips.clear();
    await traduzir("deixa o céu vermelho, como num pôr do sol");
    await log(`tradução da edição: ${est.trad.en} · tamanho ${tamanhoFinal().join("×")}`);
    const b = await gerarEsperar("editar (rascunho, foto da etapa anterior)");
    await log(`etapas: ${cadeia(b).map((x) => nomeArquivo(x.caminho)).join(" → ")}`);

    est.seedFixo = false;
    est.galeria = await invoke("galeria", { limite: 400 });
    est.sel = b.caminho;
    const c0 = Date.now();
    variar();
    await quando(() => true);
    await log(`repetir com variação: ${est.atual ? "começou" : "NÃO COMEÇOU"}`);
    if (est.atual) {
      while (est.atual) await new Promise((r) => aoFim.push(r));
      await log(`variação pronta: ${itemSel() && itemSel().nome} em ${((Date.now() - c0) / 1000).toFixed(0)} s`);
    }

    const bytes = new Uint8Array(await (await fetch(url(a.mini))).arrayBuffer());
    const colada = await invoke("colar", bytes);
    await log(`colar: ${nomeArquivo(colada)} ${(await invoke("dimensoes", { caminho: colada })).join("×")}`);
    await log("FIM OK");
  } catch (e) {
    await log(`FALHOU: ${e}`);
  }
  invoke("sair");
}

// ---------- início ----------

lembrar();
renderTudo();
T.event.listen("estado", (e) => aoEstado(e.payload));
invoke("estado").then(aoEstado).catch((e) => aviso(String(e), true));
carregarGaleria();
invoke("autoteste").then((sim) => sim && autoteste()).catch(() => {});
