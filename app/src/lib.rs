//! Backend Tauri do Revela: liga a interface (ui/app.js) ao motor.
//! Ao abrir, em segundo plano: acha o ComfyUI (usa o Desktop se estiver aberto; senão liga o do Revela, escondido)
//! e abre o tradutor. A interface acompanha pelo evento "estado".

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use base64::Engine;
use motor::comfy::{Comfy, Evento, Resultado};
use motor::fluxo::{self, Pedido, Qualidade};
use motor::galeria::{self, Ficha};
use motor::local::{self, Config};
use motor::tradutor::{self, Modo, Tradutor};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
enum Situacao {
    #[default]
    Fechado,
    Ligando,
    Ligado,
    Erro,
    /// Não instalado (sem ComfyUI Desktop ou sem llama-server).
    Ausente,
    /// Falta instalar (a interface mostra a tela "Preparar este Mac").
    Instalar,
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct EstadoApp {
    motor: Situacao,
    porta: Option<u16>,
    /// Usando o ComfyUI Desktop que já estava aberto.
    desktop: bool,
    motor_msg: String,
    tradutor: Situacao,
    tradutor_msg: String,
    faltando: Vec<String>,
    saida: String,
    entrada: String,
    modelos: String,
    comfy_dir: String,
    log: String,
    /// Segundos por passo por megapixel já medidos neste Mac (chave: "rapida-0", "caprichada-1"...).
    seg_passo: BTreeMap<String, f64>,
    favoritas: Vec<String>,
    /// O que falta instalar: "nada", "tradutor" (2,5 GB) ou "completo" (uns 28 GB).
    falta: String,
    livre_gb: u64,
    memoria_gb: u64,
    pasta_app: String,
}

#[derive(Default)]
struct Estado {
    config: Mutex<Option<Config>>,
    app: Mutex<EstadoApp>,
    motor: Mutex<Option<Comfy>>,
    proprio: Mutex<Option<Child>>,
    tradutor: Mutex<Option<Tradutor>>,
    cancelar: Arc<AtomicBool>,
    saindo: AtomicBool,
    instalando: AtomicBool,
    vigia: AtomicBool,
}

fn publicar(app: &AppHandle) {
    let e = app.state::<Estado>().app.lock().unwrap().clone();
    let _ = app.emit("estado", e);
}

fn mudar(app: &AppHandle, f: impl FnOnce(&mut EstadoApp)) {
    f(&mut app.state::<Estado>().app.lock().unwrap());
    publicar(app);
}

fn config(app: &AppHandle) -> Result<Config, String> {
    app.state::<Estado>().config.lock().unwrap().clone().ok_or_else(|| "o Revela não achou o ComfyUI Desktop".to_string())
}

/// Liga o motor: usa um ComfyUI que já esteja no ar ou abre o do Revela.
fn ligar_motor_agora(app: &AppHandle) {
    let Ok(c) = config(app) else { return };
    if let Some(m) = local::achar_motor(&c) {
        let desktop = m.porta == c.porta_desktop;
        let porta = m.porta;
        *app.state::<Estado>().motor.lock().unwrap() = Some(m);
        mudar(app, |e| {
            e.motor = Situacao::Ligado;
            e.porta = Some(porta);
            e.desktop = desktop;
            e.motor_msg.clear();
        });
        return;
    }
    mudar(app, |e| {
        e.motor = Situacao::Ligando;
        e.motor_msg = "ligando o motor…".into();
    });
    let log = local::pasta_app().join("comfyui.log");
    match local::ligar_comfy(&c, local::PORTA_REVELA, &log) {
        Ok(filho) => {
            *app.state::<Estado>().proprio.lock().unwrap() = Some(filho);
            *app.state::<Estado>().motor.lock().unwrap() = Some(Comfy::new(local::PORTA_REVELA));
            mudar(app, |e| {
                e.motor = Situacao::Ligado;
                e.porta = Some(local::PORTA_REVELA);
                e.desktop = false;
                e.motor_msg.clear();
            });
        }
        Err(err) => mudar(app, |e| {
            e.motor = Situacao::Erro;
            e.motor_msg = format!("{err:#}");
        }),
    }
}

fn ligar_tradutor_agora(app: &AppHandle) {
    let Ok(c) = config(app) else { return };
    let Some(bin) = Tradutor::achar_binario() else {
        mudar(app, |e| {
            e.tradutor = Situacao::Ausente;
            e.tradutor_msg = "não achei o llama-server (llama.cpp)".into();
        });
        return;
    };
    mudar(app, |e| e.tradutor = Situacao::Ligando);
    let mut t = Tradutor::new(bin, c.tradutor_modelo.clone(), tradutor::PORTA_PADRAO);
    let r = t.ligar(Some(&local::pasta_app().join("tradutor.log")));
    *app.state::<Estado>().tradutor.lock().unwrap() = Some(t);
    match r {
        Ok(()) => mudar(app, |e| {
            e.tradutor = Situacao::Ligado;
            e.tradutor_msg.clear();
        }),
        Err(err) => mudar(app, |e| {
            e.tradutor = Situacao::Erro;
            e.tradutor_msg = format!("{err:#}");
        }),
    }
}

/// Espaço livre (GB) no disco de uma pasta e memória do Mac (GB).
fn disco_e_memoria(pasta: &Path) -> (u64, u64) {
    let saida = |c: &str, a: &[&str]| {
        std::process::Command::new(c).args(a).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
    };
    let df = saida("/bin/df", &["-g", &pasta.display().to_string()]);
    let livre = df.lines().nth(1).and_then(|l| l.split_whitespace().nth(3)).and_then(|v| v.parse().ok()).unwrap_or(0);
    let mem = saida("/usr/sbin/sysctl", &["-n", "hw.memsize"]).trim().parse::<u64>().unwrap_or(0) / (1 << 30);
    (livre, mem)
}

/// Ao abrir o app: configuração, motor e tradutor (em paralelo), e um vigia que percebe se o motor fechou.
fn preparar(app: AppHandle) {
    let (c, falta) = local::preparar();
    let pasta = local::pasta_app();
    if let Some(c) = &c {
        let faltando = local::modelos_faltando(c);
        mudar(&app, |e| {
            e.faltando = faltando;
            e.saida = c.saida.display().to_string();
            e.entrada = c.entrada.display().to_string();
            e.modelos = c.modelos_dir.display().to_string();
            e.comfy_dir = c.comfy_dir.display().to_string();
            e.log = pasta.join("comfyui.log").display().to_string();
            e.seg_passo = c.seg_passo.clone();
            e.favoritas = c.favoritas.clone();
        });
        *app.state::<Estado>().config.lock().unwrap() = Some(c.clone());
    }
    if falta != local::Falta::Nada {
        let (livre, mem) = disco_e_memoria(&pasta);
        mudar(&app, |e| {
            e.falta = if falta == local::Falta::Tradutor { "tradutor" } else { "completo" }.into();
            e.motor = Situacao::Instalar;
            e.tradutor = Situacao::Instalar;
            e.livre_gb = livre;
            e.memoria_gb = mem;
            e.pasta_app = pasta.display().to_string();
        });
        return; // a interface mostra a tela de instalação; o comando `instalar` chama `preparar` de novo no fim
    }
    mudar(&app, |e| e.falta = "nada".into());
    let a2 = app.clone();
    std::thread::spawn(move || ligar_tradutor_agora(&a2));
    ligar_motor_agora(&app);
    if app.state::<Estado>().vigia.swap(true, Ordering::Relaxed) {
        return; // já tem um vigia rodando
    }

    // vigia: se o ComfyUI fechar (por exemplo, o Desktop), religa o do Revela
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let st = app.state::<Estado>();
        if st.saindo.load(Ordering::Relaxed) {
            return;
        }
        let ligado = st.app.lock().unwrap().motor == Situacao::Ligado;
        let m = st.motor.lock().unwrap().clone();
        if ligado && m.is_some_and(|m| !m.vivo()) {
            *st.motor.lock().unwrap() = None;
            mudar(&app, |e| {
                e.motor = Situacao::Fechado;
                e.motor_msg = "o ComfyUI fechou; ligando de novo…".into();
            });
            ligar_motor_agora(&app);
        }
    }
}

/// Roda o instalador (recursos/instalador/instalar.sh) e manda cada linha para a interface (evento "instalacao").
/// No fim, prepara o app de novo (motor e tradutor).
#[tauri::command]
fn instalar(app: AppHandle) -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    let st = app.state::<Estado>();
    if st.instalando.swap(true, Ordering::Relaxed) {
        return Ok(());
    }
    let modo = st.app.lock().unwrap().falta.clone();
    let rec = app.path().resource_dir().map_err(|e| e.to_string())?.join("instalador");
    let filho = std::process::Command::new("/bin/zsh")
        .arg(rec.join("instalar.sh"))
        .arg(&rec)
        .arg(local::pasta_app())
        .arg(if modo == "tradutor" { "tradutor" } else { "completo" })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut filho = match filho {
        Ok(f) => f,
        Err(e) => {
            st.instalando.store(false, Ordering::Relaxed);
            return Err(format!("não consegui abrir o instalador: {e}"));
        }
    };
    let saida = filho.stdout.take().unwrap();
    std::thread::spawn(move || {
        let mut ok = false;
        for linha in BufReader::new(saida).lines().map_while(Result::ok) {
            ok |= linha.trim() == "PRONTO";
            let _ = app.emit("instalacao", linha);
        }
        let _ = filho.wait();
        app.state::<Estado>().instalando.store(false, Ordering::Relaxed);
        if ok {
            preparar(app);
        } else {
            let _ = app.emit("instalacao", "FIM_SEM_SUCESSO");
        }
    });
    Ok(())
}

#[tauri::command]
fn estado(st: tauri::State<Estado>) -> EstadoApp {
    st.app.lock().unwrap().clone()
}

/// Tenta de novo (botão no selo do motor).
#[tauri::command]
async fn ligar_motor(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || ligar_motor_agora(&app)).await;
}

#[tauri::command]
async fn traduzir(app: AppHandle, pedido: String, modo: Modo, fotos: usize) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<Estado>();
        let mut t = st.tradutor.lock().unwrap();
        let t = t.as_mut().ok_or("o tradutor ainda está abrindo")?;
        t.traduzir(&pedido, modo, fotos).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PedidoUi {
    pedido: String,
    prompt: String,
    #[serde(default)]
    termos: Vec<String>,
    #[serde(default)]
    fotos: Vec<String>,
    largura: u32,
    altura: u32,
    #[serde(default)]
    rascunho: bool,
    qualidade: Qualidade,
    seed: u64,
}

#[derive(Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "camelCase")]
enum EventoUi {
    Enviando,
    #[serde(rename_all = "camelCase")]
    Enfileirado { na_frente: u32 },
    Etapa { nome: String },
    #[serde(rename_all = "camelCase")]
    Passo { valor: u32, max: u32, seg_passo: Option<f64>, faltam_s: Option<f64> },
    Previa { dados: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemUi {
    #[serde(flatten)]
    item: galeria::Item,
    mini: Option<String>,
    favorita: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FimUi {
    cancelado: bool,
    tempo_s: f64,
    item: Option<ItemUi>,
}

fn item_ui(it: galeria::Item, favoritas: &[String]) -> ItemUi {
    let mini = motor::imagem::miniatura(Path::new(&it.caminho), &local::pasta_app().join("miniaturas"))
        .ok()
        .map(|p| p.display().to_string());
    let favorita = favoritas.contains(&it.caminho);
    ItemUi { item: it, mini, favorita }
}

#[tauri::command]
async fn gerar(app: AppHandle, pedido: PedidoUi, canal: Channel<EventoUi>) -> Result<FimUi, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<Estado>();
        let c = config(&app)?;
        let m = st.motor.lock().unwrap().clone().ok_or("o motor ainda não está ligado")?;
        let cancelar = st.cancelar.clone();
        cancelar.store(false, Ordering::Relaxed);
        let _ = canal.send(EventoUi::Enviando);

        let mut refs = Vec::new();
        for f in &pedido.fotos {
            refs.push(m.enviar_imagem(Path::new(f)).map_err(|e| format!("{e:#}"))?);
        }
        let prompt = tradutor::juntar_termos(&pedido.prompt, &pedido.termos);
        let p = Pedido {
            prompt: prompt.clone(),
            referencias: refs,
            largura: pedido.largura,
            altura: pedido.altura,
            qualidade: pedido.qualidade,
            seed: pedido.seed,
            prefixo: fluxo::prefixo_padrao(),
        };
        let ficha = Ficha {
            pedido: pedido.pedido.clone(),
            prompt,
            termos: pedido.termos.clone(),
            modo: if pedido.fotos.is_empty() { "criar" } else { "editar" }.into(),
            qualidade: pedido.qualidade,
            seed: pedido.seed,
            largura: fluxo::mult16(pedido.largura as f64),
            altura: fluxo::mult16(pedido.altura as f64),
            rascunho: pedido.rascunho,
            referencias: pedido.fotos.clone(),
            versao: env!("CARGO_PKG_VERSION").into(),
        };
        let ficha_json = serde_json::to_value(&ficha).map_err(|e| e.to_string())?;
        let seg = Cell::new(None::<f64>);
        let fim = m
            .gerar(&fluxo::montar(&p, &c.modelos), Some(&ficha_json), &|| cancelar.load(Ordering::Relaxed), |e| {
                let ev = match e {
                    Evento::Enfileirado { na_frente, .. } => EventoUi::Enfileirado { na_frente },
                    Evento::Etapa { nome, .. } => EventoUi::Etapa { nome },
                    Evento::Passo { valor, max, seg_passo, faltam_s } => {
                        if seg_passo.is_some() {
                            seg.set(seg_passo);
                        }
                        EventoUi::Passo { valor, max, seg_passo, faltam_s }
                    }
                    Evento::Previa(b) => EventoUi::Previa { dados: base64::engine::general_purpose::STANDARD.encode(b) },
                };
                let _ = canal.send(ev);
            })
            .map_err(|e| format!("{e:#}"))?;

        match fim {
            Resultado::Cancelado { .. } => Ok(FimUi { cancelado: true, tempo_s: 0.0, item: None }),
            Resultado::Pronto { saidas, tempo_s, .. } => {
                // guarda o tempo medido para as próximas estimativas
                if let Some(s) = seg.get() {
                    let mp = (ficha.largura as f64 * ficha.altura as f64) / fluxo::PIXELS_PADRAO;
                    let chave = fluxo::chave_tempo(pedido.qualidade, pedido.fotos.len());
                    let medidos = st.config.lock().unwrap().as_mut().map(|cfg| {
                        let novo = s / mp.max(0.05);
                        let v = cfg.seg_passo.get(&chave).map(|a| a * 0.5 + novo * 0.5).unwrap_or(novo);
                        cfg.seg_passo.insert(chave, v);
                        let _ = local::salvar(cfg);
                        cfg.seg_passo.clone()
                    });
                    if let Some(medidos) = medidos {
                        mudar(&app, |e| e.seg_passo = medidos);
                    }
                }
                let favoritas = st.app.lock().unwrap().favoritas.clone();
                let item = saidas.first().map(|s| {
                    let p = s.caminho(&c.saida);
                    let quando = std::fs::metadata(&p)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    item_ui(galeria::item(&p, quando), &favoritas)
                });
                Ok(FimUi { cancelado: false, tempo_s, item })
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancelar(st: tauri::State<Estado>) {
    st.cancelar.store(true, Ordering::Relaxed);
}

#[tauri::command]
async fn galeria(app: AppHandle, limite: usize) -> Result<Vec<ItemUi>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let c = config(&app)?;
        let favoritas = app.state::<Estado>().app.lock().unwrap().favoritas.clone();
        Ok(galeria::listar(&c.saida, limite).into_iter().map(|it| item_ui(it, &favoritas)).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn favoritar(app: AppHandle, caminho: String, sim: bool) -> Result<(), String> {
    let st = app.state::<Estado>();
    let mut cfg = st.config.lock().unwrap();
    let cfg = cfg.as_mut().ok_or("sem configuração")?;
    cfg.favoritas.retain(|f| f != &caminho);
    if sim {
        cfg.favoritas.push(caminho);
    }
    local::salvar(cfg).map_err(|e| e.to_string())?;
    let fav = cfg.favoritas.clone();
    st.app.lock().unwrap().favoritas = fav;
    Ok(())
}

#[tauri::command]
fn dimensoes(caminho: String) -> Result<(u32, u32), String> {
    motor::imagem::dimensoes(Path::new(&caminho)).map_err(|e| format!("{e:#}"))
}

/// Imagem colada (⌘V): os bytes vêm crus da interface e vão para input/revela.
#[tauri::command]
fn colar(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(dados) = request.body() else {
        return Err("a imagem colada não chegou".into());
    };
    let c = config(&app)?;
    let ext = if dados.starts_with(&[0xff, 0xd8]) { "jpg" } else { "png" };
    let pasta = c.entrada.join("revela");
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    let p = pasta.join(format!("colada_{:012x}.{ext}", motor::fnv64(dados) >> 16));
    std::fs::write(&p, dados).map_err(|e| e.to_string())?;
    Ok(p.display().to_string())
}

#[tauri::command]
async fn para_avid(app: AppHandle, caminho: String, preencher: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let c = config(&app)?;
        motor::imagem::para_avid(Path::new(&caminho), &c.saida.join(galeria::PASTA_AVID), preencher)
            .map(|p| p.display().to_string())
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Abre uma pasta ou arquivo no Finder (`revelar` = selecionar o arquivo na pasta).
#[tauri::command]
fn mostrar(caminho: String, revelar: bool) -> Result<(), String> {
    let mut cmd = std::process::Command::new("/usr/bin/open");
    if revelar {
        cmd.arg("-R");
    }
    let p = PathBuf::from(&caminho);
    if !p.exists() {
        return Err(format!("não achei {caminho}"));
    }
    cmd.arg(p).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Autoteste (REVELA_AUTOTESTE=1): a janela fica escondida, a interface faz o caminho todo e grava um registro.
#[tauri::command]
fn autoteste() -> bool {
    std::env::var_os("REVELA_AUTOTESTE").is_some()
}

#[tauri::command]
fn registrar(linha: String) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(local::pasta_app().join("autoteste.log")) {
        let _ = writeln!(f, "{linha}");
    }
}

#[tauri::command]
fn sair(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Estado::default())
        .setup(|app| {
            if autoteste() {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            let h = app.handle().clone();
            std::thread::spawn(move || preparar(h));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            estado, ligar_motor, traduzir, gerar, cancelar, galeria, favoritar, dimensoes, colar, para_avid, mostrar, instalar, autoteste,
            registrar, sair
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo")
        .run(|app, ev| {
            if let tauri::RunEvent::Exit = ev {
                let st = app.state::<Estado>();
                st.saindo.store(true, Ordering::Relaxed);
                let proprio = st.proprio.lock().unwrap().take();
                if let Some(mut p) = proprio {
                    let _ = p.kill();
                    let _ = p.wait();
                }
                let tradutor = st.tradutor.lock().unwrap().take();
                if let Some(mut t) = tradutor {
                    t.desligar();
                }
            }
        });
}
