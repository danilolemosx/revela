//! Onde estão as coisas no Mac e como ligar o motor.
//!
//! O Revela usa a instalação do ComfyUI Desktop (Python, ComfyUI e modelos). Se o Desktop já estiver aberto, o app
//! usa ele (porta 8188). Senão, liga o ComfyUI escondido na porta 8189, só em 127.0.0.1, em modo `--offline`
//! (sem acesso à internet e sem os nós pagos) e sem o ComfyUI-Manager.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::comfy::Comfy;
use crate::fluxo::Modelos;

pub const PORTA_DESKTOP: u16 = 8188;
pub const PORTA_REVELA: u16 = 8189;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Pasta do ComfyUI (a que tem o main.py).
    pub comfy_dir: PathBuf,
    pub python: PathBuf,
    /// extra_model_paths.yaml gerado pelo Desktop (aponta para a pasta de modelos).
    pub caminhos_modelos: Option<PathBuf>,
    pub modelos_dir: PathBuf,
    pub entrada: PathBuf,
    pub saida: PathBuf,
    /// Porta do ComfyUI Desktop, quando ele está aberto.
    pub porta_desktop: u16,
    pub tradutor_modelo: PathBuf,
    #[serde(default)]
    pub modelos: Modelos,
    #[serde(default)]
    pub favoritas: Vec<String>,
    /// Segundos por passo já medidos, por qualidade ("rapida"/"caprichada"), para 1024×1024.
    #[serde(default)]
    pub seg_passo: BTreeMap<String, f64>,
}

/// ~/Library/Application Support/Revela
pub fn pasta_app() -> PathBuf {
    let p = dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("Revela");
    let _ = std::fs::create_dir_all(&p);
    p
}

fn arquivo_config() -> PathBuf {
    pasta_app().join("config.json")
}

/// A configuração salva; na primeira vez, descobre a partir do ComfyUI Desktop.
pub fn carregar() -> Result<Config> {
    if let Ok(t) = std::fs::read_to_string(arquivo_config()) {
        if let Ok(c) = serde_json::from_str::<Config>(&t) {
            if c.comfy_dir.join("main.py").is_file() {
                return Ok(c);
            }
        }
    }
    let c = descobrir()?;
    salvar(&c)?;
    Ok(c)
}

pub fn salvar(c: &Config) -> Result<()> {
    std::fs::write(arquivo_config(), serde_json::to_string_pretty(c)?)?;
    Ok(())
}

fn ler_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// Lê a configuração do ComfyUI Desktop (instalação, pastas e porta).
pub fn descobrir() -> Result<Config> {
    let base = dirs::data_dir().ok_or_else(|| anyhow!("sem pasta de dados"))?.join("Comfy Desktop");
    let inst = ler_json(&base.join("installations.json"))
        .ok_or_else(|| anyhow!("não achei o ComfyUI Desktop ({}/installations.json)", base.display()))?;
    let lista = inst.as_array().cloned().unwrap_or_default();
    let ultima = ler_json(&base.join("last-session.json")).and_then(|v| v["installationId"].as_str().map(String::from));
    let local = |i: &&Value| i["installPath"].as_str().is_some() && i["sourceId"].as_str() != Some("cloud");
    let escolhida = lista
        .iter()
        .filter(local)
        .find(|i| ultima.as_deref().is_some_and(|u| i["id"].as_str() == Some(u)))
        .or_else(|| lista.iter().find(local))
        .ok_or_else(|| anyhow!("o ComfyUI Desktop não tem nenhuma instalação local"))?;
    let id = escolhida["id"].as_str().unwrap_or("");
    let install = PathBuf::from(escolhida["installPath"].as_str().unwrap_or(""));
    let comfy_dir = install.join("ComfyUI");
    if !comfy_dir.join("main.py").is_file() {
        bail!("não achei o ComfyUI em {}", comfy_dir.display());
    }
    let python = comfy_dir.join(".venv/bin/python");
    let ajustes = ler_json(&base.join("settings.json")).unwrap_or(Value::Null);
    let modelos_dir = ajustes["modelsDirs"][0].as_str().map(PathBuf::from).unwrap_or_else(|| comfy_dir.join("models"));
    let entrada = ajustes["inputDir"].as_str().map(PathBuf::from).unwrap_or_else(|| comfy_dir.join("input"));
    let saida = ajustes["outputDir"].as_str().map(PathBuf::from).unwrap_or_else(|| comfy_dir.join("output"));
    let yaml = base.join("instance-model-paths").join(format!("{id}.yaml"));
    let porta_desktop = ler_json(&base.join("comfy-procs").join(format!("{id}.json")))
        .and_then(|v| v["port"].as_u64())
        .map(|p| p as u16)
        .unwrap_or(PORTA_DESKTOP);
    Ok(Config {
        comfy_dir,
        python,
        caminhos_modelos: yaml.is_file().then_some(yaml),
        tradutor_modelo: modelos_dir.join("llm").join(crate::tradutor::MODELO_PADRAO),
        modelos_dir,
        entrada,
        saida,
        porta_desktop,
        modelos: Modelos::default(),
        favoritas: vec![],
        seg_passo: BTreeMap::new(),
    })
}

/// Modelos que faltam na pasta (nome → pasta onde deveria estar).
pub fn modelos_faltando(c: &Config) -> Vec<String> {
    let m = &c.modelos;
    [
        ("diffusion_models", &m.rapida),
        ("diffusion_models", &m.caprichada),
        ("text_encoders", &m.texto),
        ("vae", &m.vae),
    ]
    .iter()
    .filter(|(pasta, nome)| !c.modelos_dir.join(pasta).join(nome).is_file())
    .map(|(pasta, nome)| format!("{pasta}/{nome}"))
    .collect()
}

/// Qual ComfyUI está no ar: primeiro o Desktop, depois o do Revela.
pub fn achar_motor(c: &Config) -> Option<Comfy> {
    [c.porta_desktop, PORTA_REVELA].into_iter().map(Comfy::new).find(|m| m.vivo())
}

/// Liga o ComfyUI escondido e espera ele responder (o primeiro início leva de 20 s a 1 min).
pub fn ligar_comfy(c: &Config, porta: u16, log: &Path) -> Result<Child> {
    let mut cmd = Command::new(&c.python);
    cmd.current_dir(&c.comfy_dir)
        .arg("main.py")
        .args(["--listen", "127.0.0.1", "--port", &porta.to_string()])
        .args(["--offline", "--disable-auto-launch"])
        .arg("--input-directory")
        .arg(&c.entrada)
        .arg("--output-directory")
        .arg(&c.saida);
    if let Some(y) = &c.caminhos_modelos {
        cmd.arg("--extra-model-paths-config").arg(y);
    }
    let arq = std::fs::File::create(log).with_context(|| format!("não consegui criar {}", log.display()))?;
    let mut filho = cmd
        .env("PYTORCH_ENABLE_MPS_FALLBACK", "1")
        .env("DO_NOT_TRACK", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(arq.try_clone()?))
        .stderr(Stdio::from(arq))
        .spawn()
        .with_context(|| format!("não consegui abrir o Python do ComfyUI ({})", c.python.display()))?;
    let m = Comfy::new(porta);
    let inicio = Instant::now();
    while inicio.elapsed() < Duration::from_secs(240) {
        if m.vivo() {
            return Ok(filho);
        }
        if let Ok(Some(st)) = filho.try_wait() {
            let fim = std::fs::read_to_string(log).unwrap_or_default();
            let ultimas: Vec<&str> = fim.lines().rev().take(6).collect();
            bail!("o ComfyUI fechou ao abrir ({st}): {}", ultimas.into_iter().rev().collect::<Vec<_>>().join(" | "));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = filho.kill();
    bail!("o ComfyUI demorou demais para abrir")
}
