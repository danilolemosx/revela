//! Conversa com o ComfyUI local pela API HTTP (127.0.0.1) e pelo websocket de progresso.
//! Nada sai do computador: só endereços 127.0.0.1.

use std::io::{ErrorKind, Read};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

/// O que acontece durante uma geração (vai para a interface).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "tipo", rename_all = "camelCase")]
pub enum Evento {
    /// Pedido aceito; `na_frente` = quantos pedidos estão antes dele na fila do ComfyUI.
    Enfileirado { id: String, na_frente: u32 },
    /// Mudou de etapa (lendo o pedido, carregando o modelo, gerando...).
    Etapa { no: String, nome: String },
    /// Um passo do amostrador. `seg_passo` e `faltam_s` aparecem a partir do segundo passo.
    Passo { valor: u32, max: u32, seg_passo: Option<f64>, faltam_s: Option<f64> },
    /// Prévia ao vivo (JPEG ou PNG em base64 fica com a interface; aqui vão os bytes).
    #[serde(skip)]
    Previa(Vec<u8>),
}

/// Uma imagem gerada.
#[derive(Debug, Clone, Serialize)]
pub struct Saida {
    pub arquivo: String,
    pub subpasta: String,
    pub tipo: String,
}

/// Como terminou uma geração.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "fim", rename_all = "camelCase")]
pub enum Resultado {
    Pronto { id: String, saidas: Vec<Saida>, tempo_s: f64 },
    Cancelado { id: String },
}

#[derive(Debug, Clone)]
pub struct Comfy {
    pub porta: u16,
    agente: ureq::Agent,
}

impl Comfy {
    pub fn new(porta: u16) -> Self {
        let agente = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_millis(800))
            .timeout(Duration::from_secs(120))
            .build();
        Comfy { porta, agente }
    }

    fn url(&self, caminho: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.porta, caminho)
    }

    fn get_json(&self, caminho: &str) -> Result<Value> {
        let r = self.agente.get(&self.url(caminho)).call().map_err(|e| anyhow!("ComfyUI {caminho}: {e}"))?;
        Ok(r.into_json()?)
    }

    fn post_json(&self, caminho: &str, corpo: Value) -> Result<Value> {
        match self.agente.post(&self.url(caminho)).send_json(corpo) {
            Ok(r) => Ok(r.into_json().unwrap_or(Value::Null)),
            Err(ureq::Error::Status(codigo, r)) => {
                let texto = r.into_string().unwrap_or_default();
                bail!("ComfyUI {caminho} respondeu {codigo}: {}", resumir_erro(&texto))
            }
            Err(e) => Err(anyhow!("ComfyUI {caminho}: {e}")),
        }
    }

    /// O ComfyUI está respondendo nesta porta?
    pub fn vivo(&self) -> bool {
        let a = ureq::AgentBuilder::new().timeout(Duration::from_millis(1500)).build();
        a.get(&self.url("/system_stats")).call().map(|r| r.status() == 200).unwrap_or(false)
    }

    /// Versão, memória e aparelho (MPS).
    pub fn info(&self) -> Result<Value> {
        self.get_json("/system_stats")
    }

    /// Arquivos que o ComfyUI enxerga numa pasta de modelos (ex.: "diffusion_models").
    pub fn modelos(&self, pasta: &str) -> Result<Vec<String>> {
        let v = self.get_json(&format!("/models/{pasta}"))?;
        Ok(v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default())
    }

    /// Envia uma foto de referência ao input do ComfyUI e devolve o nome para o LoadImage.
    /// O nome vem do conteúdo, então mandar a mesma foto duas vezes não duplica nada.
    pub fn enviar_imagem(&self, caminho: &Path) -> Result<String> {
        let dados = std::fs::read(caminho).with_context(|| format!("não consegui ler {}", caminho.display()))?;
        let ext = caminho.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let nome = format!("ref_{:016x}.{ext}", crate::fnv64(&dados));
        let fronteira = format!("----revela{:016x}", crate::fnv64(nome.as_bytes()));
        let mut corpo = Vec::with_capacity(dados.len() + 512);
        let campo = |corpo: &mut Vec<u8>, nome: &str, valor: &str| {
            corpo.extend(format!("--{fronteira}\r\nContent-Disposition: form-data; name=\"{nome}\"\r\n\r\n{valor}\r\n").bytes());
        };
        corpo.extend(
            format!(
                "--{fronteira}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{nome}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .bytes(),
        );
        corpo.extend(&dados);
        corpo.extend(b"\r\n");
        campo(&mut corpo, "subfolder", "revela");
        campo(&mut corpo, "overwrite", "true");
        corpo.extend(format!("--{fronteira}--\r\n").bytes());
        let r = self
            .agente
            .post(&self.url("/upload/image"))
            .set("Content-Type", &format!("multipart/form-data; boundary={fronteira}"))
            .send_bytes(&corpo)
            .map_err(|e| anyhow!("não consegui enviar a foto ao ComfyUI: {e}"))?;
        let v: Value = r.into_json()?;
        let nome = v["name"].as_str().ok_or_else(|| anyhow!("resposta inesperada do /upload/image: {v}"))?;
        let sub = v["subfolder"].as_str().unwrap_or("");
        Ok(if sub.is_empty() { nome.to_string() } else { format!("{sub}/{nome}") })
    }

    /// Põe o workflow na fila. `ficha` vai gravada dentro do PNG (texto "revela").
    pub fn enfileirar(&self, fluxo: &Value, cliente: &str, ficha: Option<&Value>) -> Result<(String, u32)> {
        let mut extra = json!({ "preview_method": "latent2rgb" });
        if let Some(f) = ficha {
            extra["extra_pnginfo"] = json!({ "revela": f });
        }
        let v = self.post_json("/prompt", json!({ "prompt": fluxo, "client_id": cliente, "extra_data": extra }))?;
        let id = v["prompt_id"].as_str().ok_or_else(|| anyhow!("o ComfyUI não devolveu o prompt_id: {v}"))?;
        Ok((id.to_string(), v["number"].as_u64().unwrap_or(0) as u32))
    }

    /// Cancela um pedido: tira da fila se ainda não começou e interrompe se já está gerando
    /// (só esse pedido, não o que outra pessoa estiver fazendo no ComfyUI).
    pub fn cancelar(&self, id: &str) -> Result<()> {
        self.tirar_da_fila(&[id.to_string()])?;
        self.post_json("/interrupt", json!({ "prompt_id": id })).map(|_| ())
    }

    /// Tira pedidos que ainda não começaram da fila do ComfyUI.
    pub fn tirar_da_fila(&self, ids: &[String]) -> Result<()> {
        self.post_json("/queue", json!({ "delete": ids })).map(|_| ())
    }

    /// Quantos pedidos estão na fila (rodando + esperando).
    pub fn tamanho_fila(&self) -> Result<u32> {
        let v = self.get_json("/queue")?;
        let n = |k: &str| v[k].as_array().map(|a| a.len()).unwrap_or(0) as u32;
        Ok(n("queue_running") + n("queue_pending"))
    }

    pub fn historico(&self, id: &str) -> Result<Option<Value>> {
        let v = self.get_json(&format!("/history/{id}"))?;
        Ok(v.get(id).cloned())
    }

    /// Baixa uma imagem do ComfyUI (/view).
    pub fn baixar(&self, s: &Saida) -> Result<Vec<u8>> {
        let url = format!(
            "{}?filename={}&subfolder={}&type={}",
            self.url("/view"),
            codificar(&s.arquivo),
            codificar(&s.subpasta),
            codificar(&s.tipo)
        );
        let r = self.agente.get(&url).call().map_err(|e| anyhow!("ComfyUI /view: {e}"))?;
        let mut v = Vec::new();
        r.into_reader().read_to_end(&mut v)?;
        Ok(v)
    }

    /// Envia o workflow e acompanha até o fim pelo websocket, chamando `avisar` a cada evento.
    /// `cancelado` é consultado a cada segundo: quando ficar verdadeiro, o pedido é cancelado.
    pub fn gerar(
        &self,
        fluxo: &Value,
        ficha: Option<&Value>,
        cancelado: &dyn Fn() -> bool,
        mut avisar: impl FnMut(Evento),
    ) -> Result<Resultado> {
        let cliente = crate::id_unico();
        // o websocket abre antes de enfileirar, para não perder nenhum evento
        let tcp = TcpStream::connect(("127.0.0.1", self.porta)).context("o ComfyUI não está respondendo")?;
        let (mut ws, _) = tungstenite::client(format!("ws://127.0.0.1:{}/ws?clientId={cliente}", self.porta), tcp)
            .map_err(|e| anyhow!("websocket do ComfyUI: {e}"))?;
        ws.get_ref().set_read_timeout(Some(Duration::from_secs(1)))?;

        let inicio = Instant::now();
        let (id, _) = self.enfileirar(fluxo, &cliente, ficha)?;
        let na_frente = self.tamanho_fila().unwrap_or(1).saturating_sub(1);
        avisar(Evento::Enfileirado { id: id.clone(), na_frente });

        let mut relogio = Relogio::default();
        let mut sem_resposta = 0u32;
        let (mut comecou, mut pediu_cancelar) = (false, false);
        // o "execution_success" sai antes de o ComfyUI gravar o histórico; o fim de verdade é o "executing" sem nó
        let mut sucesso = false;
        loop {
            if !pediu_cancelar && cancelado() {
                pediu_cancelar = true;
                self.cancelar(&id)?;
                if !comecou {
                    return Ok(Resultado::Cancelado { id });
                }
            }
            let msg = match ws.read() {
                Ok(m) => {
                    sem_resposta = 0;
                    m
                }
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    if sucesso {
                        break;
                    }
                    sem_resposta += 1;
                    // a cada ~15 s sem nada, confere se o ComfyUI ainda está aberto
                    if sem_resposta % 15 == 0 && !self.vivo() {
                        bail!("o ComfyUI fechou durante a geração");
                    }
                    continue;
                }
                Err(e) => bail!("o websocket do ComfyUI caiu: {e}"),
            };
            match msg {
                tungstenite::Message::Binary(b) => {
                    if let Some(img) = previa(&b) {
                        avisar(Evento::Previa(img));
                    }
                }
                tungstenite::Message::Text(t) => {
                    let v: Value = match serde_json::from_str(&t) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let d = &v["data"];
                    if d.get("prompt_id").and_then(|p| p.as_str()).is_some_and(|p| p != id) {
                        continue; // de outro pedido (por exemplo, alguém usando o ComfyUI ao mesmo tempo)
                    }
                    let tipo = v["type"].as_str().unwrap_or("");
                    if d.get("prompt_id").is_some() && tipo != "status" {
                        comecou = true;
                    }
                    match tipo {
                        "executing" => match d["node"].as_str() {
                            Some(no) => avisar(Evento::Etapa { no: no.to_string(), nome: crate::fluxo::etapa(no).to_string() }),
                            None if d.get("prompt_id").is_some() => break,
                            None => {}
                        },
                        "progress" => {
                            let (valor, max) = (d["value"].as_u64().unwrap_or(0) as u32, d["max"].as_u64().unwrap_or(1) as u32);
                            let (seg_passo, faltam_s) = relogio.passo(valor, max);
                            avisar(Evento::Passo { valor, max, seg_passo, faltam_s });
                        }
                        "execution_success" => sucesso = true,
                        "execution_interrupted" => return Ok(Resultado::Cancelado { id }),
                        "execution_error" => {
                            let msg = d["exception_message"].as_str().unwrap_or("erro desconhecido");
                            let no = d["node_type"].as_str().unwrap_or("");
                            bail!("o ComfyUI deu erro em {no}: {}", msg.trim());
                        }
                        _ => {}
                    }
                }
                tungstenite::Message::Close(_) => bail!("o ComfyUI fechou a conexão"),
                _ => {}
            }
        }
        let _ = ws.close(None);

        let mut h = None;
        for _ in 0..50 {
            h = self.historico(&id)?;
            if h.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let h = h.ok_or_else(|| anyhow!("o pedido {id} não apareceu no histórico do ComfyUI"))?;
        let mut saidas = Vec::new();
        if let Some(outs) = h["outputs"].as_object() {
            for o in outs.values() {
                for img in o["images"].as_array().into_iter().flatten() {
                    saidas.push(Saida {
                        arquivo: img["filename"].as_str().unwrap_or("").into(),
                        subpasta: img["subfolder"].as_str().unwrap_or("").into(),
                        tipo: img["type"].as_str().unwrap_or("output").into(),
                    });
                }
            }
        }
        Ok(Resultado::Pronto { id, saidas, tempo_s: inicio.elapsed().as_secs_f64() })
    }
}

impl Saida {
    /// Caminho do arquivo, sabendo a pasta output do ComfyUI.
    pub fn caminho(&self, output: &Path) -> PathBuf {
        let mut p = output.to_path_buf();
        if !self.subpasta.is_empty() {
            p.push(&self.subpasta);
        }
        p.push(&self.arquivo);
        p
    }
}

/// Mede o tempo por passo e estima quanto falta.
#[derive(Default)]
struct Relogio {
    primeiro: Option<(u32, Instant)>,
}

impl Relogio {
    fn passo(&mut self, valor: u32, max: u32) -> (Option<f64>, Option<f64>) {
        let agora = Instant::now();
        match self.primeiro {
            Some((v0, t0)) if valor > v0 => {
                let seg = agora.duration_since(t0).as_secs_f64() / (valor - v0) as f64;
                (Some(seg), Some(seg * max.saturating_sub(valor) as f64))
            }
            Some(_) => (None, None),
            None => {
                self.primeiro = Some((valor, agora));
                (None, None)
            }
        }
    }
}

/// Mensagem binária do websocket → imagem da prévia (tipos 1 e 4 do protocolo do ComfyUI).
fn previa(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() < 8 {
        return None;
    }
    let tipo = u32::from_be_bytes(b[0..4].try_into().ok()?);
    match tipo {
        1 => Some(b[8..].to_vec()),
        4 => {
            let n = u32::from_be_bytes(b[4..8].try_into().ok()?) as usize;
            b.get(8 + n..).map(|x| x.to_vec())
        }
        _ => None,
    }
}

/// Erros de validação do /prompt viram uma frase curta.
fn resumir_erro(texto: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(texto) {
        let mut partes = vec![v["error"]["message"].as_str().unwrap_or("").to_string()];
        if let Some(nos) = v["node_errors"].as_object() {
            for (no, e) in nos {
                for err in e["errors"].as_array().into_iter().flatten() {
                    partes.push(format!(
                        "{no}: {} {}",
                        err["message"].as_str().unwrap_or(""),
                        err["details"].as_str().unwrap_or("")
                    ));
                }
            }
        }
        return partes.join(" · ");
    }
    texto.chars().take(300).collect()
}

fn codificar(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => r.push(b as char),
            _ => r.push_str(&format!("%{b:02X}")),
        }
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn previa_tipo_1() {
        let mut b = 1u32.to_be_bytes().to_vec();
        b.extend(1u32.to_be_bytes());
        b.extend([0xff, 0xd8, 0xff]);
        assert_eq!(previa(&b), Some(vec![0xff, 0xd8, 0xff]));
    }

    #[test]
    fn previa_tipo_4() {
        let meta = br#"{"image_type":"image/jpeg"}"#;
        let mut b = 4u32.to_be_bytes().to_vec();
        b.extend((meta.len() as u32).to_be_bytes());
        b.extend(meta);
        b.extend([1, 2, 3]);
        assert_eq!(previa(&b), Some(vec![1, 2, 3]));
    }

    #[test]
    fn erro_de_validacao() {
        let t = r#"{"error":{"message":"Prompt outputs failed validation"},"node_errors":{"unet":{"errors":[{"message":"Value not in list","details":"unet_name: 'x' not in []"}]}}}"#;
        assert!(resumir_erro(t).contains("unet: Value not in list"));
    }

    #[test]
    fn codifica_url() {
        assert_eq!(codificar("REVELA_00001_.png"), "REVELA_00001_.png");
        assert_eq!(codificar("a b/ç"), "a%20b%2F%C3%A7");
    }
}
