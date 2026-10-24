//! Tradutor local: português → prompt em inglês, com o Qwen3 4B Instruct (Apache 2.0) no llama.cpp.
//! O `llama-server` roda só em 127.0.0.1, sem a página web, e fica aberto enquanto o app estiver aberto
//! (carregar o modelo leva uns 20 s; depois cada tradução leva 1 ou 2 s).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PORTA_PADRAO: u16 = 8190;
pub const MODELO_PADRAO: &str = "Qwen3-4B-Instruct-2507-Q4_K_M.gguf";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modo {
    /// Fiel ao pedido, sem acrescentar nada.
    Traduzir,
    /// Acrescenta luz, lente, enquadramento e textura, sem mudar a cena.
    Detalhar,
}

const BASE: &str = "You turn requests written in Brazilian Portuguese into English prompts for the FLUX.2 image model.
Rules:
- Keep every subject, object, color, place, action and style the user asked for.
- Write natural, concrete English in one paragraph. No quotes, lists, titles or explanations. Output only the prompt.
- Film terms: plano geral or plano aberto = wide shot; plano conjunto = full shot; plano médio = medium shot; plano americano = cowboy shot; meio primeiro plano = medium close-up; primeiro plano or close = close-up; primeiríssimo plano or superclose = extreme close-up; plano detalhe = macro detail shot; plongée = high-angle shot; contra-plongée = low-angle shot; zenital = top-down overhead shot; câmera na mão = handheld camera; contraluz = backlit; hora mágica or hora dourada = golden hour; hora azul = blue hour; noite americana = day-for-night.
- Brazilian things keep their name with a short English hint, for example: pamonha (Brazilian sweet corn tamale wrapped in corn husk), a Volkswagen Kombi van, orelhão (Brazilian public phone booth).
- Brazilian Portuguese words: ouriço = hedgehog (porco-espinho = porcupine); senhor = older man; moça = young woman; carro de boi = ox cart; boteco = small Brazilian bar; café coado no pano = coffee brewed through a cloth filter.
- 'deixa X Y' or 'deixe X Y' means make X Y (deixa o asfalto molhado = make the asphalt wet); 'tira X' means remove X; 'troca X por Y' means replace X with Y; 'põe' or 'coloca' means put or add.
- If the request is already in English, return it unchanged.";

const FIEL: &str = "- Translate faithfully and add nothing: no lighting, lens, weather, framing, mood or details that the user did not ask for.";

const DETALHE: &str = "- First write a faithful translation of the whole request, keeping every element (who, what they are doing, where, when). Then enrich it like a cinematographer describing a film still: add concrete lighting, lens and framing, materials and atmosphere that fit the request. Never add people, objects or text that the user did not ask for, and never contradict the request. 50 to 110 words.";

const EDICAO: &str = "- This is an edit of reference photos. 'foto 1' or 'primeira foto' is image 1; 'foto 2' or 'segunda foto' is image 2; a single photo is image 1. Write a direct instruction, for example: Replace the sky in image 1 with a stormy overcast sky. Say only what changes; everything else in image 1 stays exactly as it is.";

const EDICAO_DETALHE: &str = "- First write the faithful instruction. Then add detail only to the new or changed elements, never to the rest of the photo. 30 to 80 words.";

/// O texto de instruções para o modelo, conforme o modo e se é edição.
pub fn instrucoes(modo: Modo, fotos: usize) -> String {
    let mut s = String::from(BASE);
    let mut linha = |l: &str| {
        s.push('\n');
        s.push_str(l);
    };
    match (modo, fotos > 0) {
        (Modo::Traduzir, false) => linha(FIEL),
        (Modo::Detalhar, false) => linha(DETALHE),
        (Modo::Traduzir, true) => {
            linha(EDICAO);
            linha(FIEL);
        }
        (Modo::Detalhar, true) => {
            linha(EDICAO);
            linha(EDICAO_DETALHE);
        }
    }
    s
}

/// Junta os termos de câmera (botões) ao fim do prompt, sem repetir o que já está lá.
pub fn juntar_termos(prompt: &str, termos: &[String]) -> String {
    let base = prompt.trim().trim_end_matches(['.', ',', ' ']).to_string();
    let baixo = base.to_lowercase();
    let novos: Vec<&str> = termos.iter().map(|t| t.trim()).filter(|t| !t.is_empty() && !baixo.contains(&t.to_lowercase())).collect();
    if novos.is_empty() {
        return if base.is_empty() { base } else { format!("{base}.") };
    }
    if base.is_empty() {
        return format!("{}.", novos.join(", "));
    }
    format!("{base}. {}.", primeira_maiuscula(&novos.join(", ")))
}

fn primeira_maiuscula(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Limpa o que o modelo às vezes acrescenta (aspas, "Prompt:").
fn limpar(t: &str) -> String {
    let mut s = t.trim();
    for p in ["Prompt:", "prompt:", "English prompt:", "Output:"] {
        if let Some(r) = s.strip_prefix(p) {
            s = r.trim();
        }
    }
    s.trim_matches(|c| c == '"' || c == '“' || c == '”').trim().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub struct Tradutor {
    pub porta: u16,
    pub binario: PathBuf,
    pub modelo: PathBuf,
    filho: Option<Child>,
}

impl Tradutor {
    pub fn new(binario: PathBuf, modelo: PathBuf, porta: u16) -> Self {
        Tradutor { porta, binario, modelo, filho: None }
    }

    /// O `llama-server` ao lado do app (versão empacotada) ou o do Homebrew.
    pub fn achar_binario() -> Option<PathBuf> {
        let mut lugares = Vec::new();
        if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            lugares.push(d.join("llama-server"));
            lugares.push(d.join("../Resources/llama/llama-server"));
        }
        lugares.push(crate::local::pasta_app().join("llama/llama-server"));
        lugares.push("/opt/homebrew/bin/llama-server".into());
        lugares.push("/usr/local/bin/llama-server".into());
        lugares.into_iter().find(|p| p.is_file())
    }

    fn url(&self, caminho: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.porta, caminho)
    }

    pub fn vivo(&self) -> bool {
        let a = ureq::AgentBuilder::new().timeout(Duration::from_millis(1000)).build();
        a.get(&self.url("/health")).call().map(|r| r.status() == 200).unwrap_or(false)
    }

    /// Liga o servidor (se ainda não estiver no ar) e espera o modelo carregar.
    pub fn ligar(&mut self, log: Option<&Path>) -> Result<()> {
        if self.vivo() {
            return Ok(());
        }
        if !self.modelo.is_file() {
            bail!("o modelo do tradutor não está em {}", self.modelo.display());
        }
        let saida = match log {
            Some(l) => Stdio::from(std::fs::File::create(l)?),
            None => Stdio::null(),
        };
        let filho = Command::new(&self.binario)
            .arg("-m")
            .arg(&self.modelo)
            .args(["--host", "127.0.0.1", "--port", &self.porta.to_string()])
            .args(["-c", "4096", "-ngl", "99", "--no-webui", "--parallel", "1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(saida)
            .spawn()
            .with_context(|| format!("não consegui abrir {}", self.binario.display()))?;
        self.filho = Some(filho);
        let inicio = Instant::now();
        while inicio.elapsed() < Duration::from_secs(120) {
            if self.vivo() {
                return Ok(());
            }
            if let Some(f) = self.filho.as_mut() {
                if let Ok(Some(st)) = f.try_wait() {
                    self.filho = None;
                    bail!("o tradutor fechou ao abrir ({st})");
                }
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        bail!("o tradutor demorou demais para abrir")
    }

    pub fn desligar(&mut self) {
        if let Some(mut f) = self.filho.take() {
            let _ = f.kill();
            let _ = f.wait();
        }
    }

    /// Português → inglês. `fotos` = quantas fotos de referência (0 = criar do zero).
    pub fn traduzir(&mut self, pedido: &str, modo: Modo, fotos: usize) -> Result<String> {
        if pedido.trim().is_empty() {
            return Ok(String::new());
        }
        self.ligar(None)?;
        let corpo = json!({
            "messages": [
                { "role": "system", "content": instrucoes(modo, fotos) },
                { "role": "user", "content": pedido.trim() }
            ],
            "temperature": if modo == Modo::Traduzir { 0.15 } else { 0.5 },
            "max_tokens": 400,
            "cache_prompt": true
        });
        let a = ureq::AgentBuilder::new().timeout(Duration::from_secs(90)).build();
        let v: Value = a
            .post(&self.url("/v1/chat/completions"))
            .send_json(corpo)
            .map_err(|e| anyhow!("o tradutor não respondeu: {e}"))?
            .into_json()?;
        let t = v["choices"][0]["message"]["content"].as_str().ok_or_else(|| anyhow!("resposta inesperada do tradutor: {v}"))?;
        Ok(limpar(t))
    }
}

impl Drop for Tradutor {
    fn drop(&mut self) {
        self.desligar();
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn termos_no_fim_sem_repetir() {
        let t = vec!["medium shot".to_string(), "50mm lens".to_string()];
        assert_eq!(juntar_termos("A hedgehog on a bar counter.", &t), "A hedgehog on a bar counter. Medium shot, 50mm lens.");
        assert_eq!(juntar_termos("A medium shot of a hedgehog", &t), "A medium shot of a hedgehog. 50mm lens.");
        assert_eq!(juntar_termos("A hedgehog", &[]), "A hedgehog.");
        assert_eq!(juntar_termos("", &t), "medium shot, 50mm lens.");
    }

    #[test]
    fn limpa_resposta() {
        assert_eq!(limpar("Prompt: \"A hedgehog\n at night\""), "A hedgehog at night");
    }

    #[test]
    fn instrucoes_de_edicao() {
        assert!(instrucoes(Modo::Traduzir, 1).contains("image 1"));
        assert!(instrucoes(Modo::Traduzir, 0).contains("add nothing"));
        assert!(instrucoes(Modo::Detalhar, 0).contains("cinematographer"));
    }
}
