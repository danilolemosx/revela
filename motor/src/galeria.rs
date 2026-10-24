//! Galeria: os resultados da pasta output, do mais novo para o mais velho, com a ficha de cada um.
//! A ficha do Revela fica dentro do PNG (texto "revela"). Para imagens feitas direto no ComfyUI, o seed e o
//! texto saem do workflow que o ComfyUI grava junto (texto "prompt").

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fluxo::Qualidade;

/// Tudo o que é preciso para refazer uma imagem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ficha {
    /// O pedido em português, como foi escrito.
    #[serde(default)]
    pub pedido: String,
    /// O que foi para a IA (inglês, já com os termos de câmera).
    pub prompt: String,
    #[serde(default)]
    pub termos: Vec<String>,
    /// "criar" ou "editar".
    #[serde(default)]
    pub modo: String,
    pub qualidade: Qualidade,
    pub seed: u64,
    pub largura: u32,
    pub altura: u32,
    #[serde(default)]
    pub rascunho: bool,
    /// Caminhos das fotos de referência (originais, no disco).
    #[serde(default)]
    pub referencias: Vec<String>,
    #[serde(default)]
    pub versao: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub caminho: String,
    pub nome: String,
    /// Segundos desde 1970 (para a interface mostrar a hora).
    pub modificado: f64,
    pub largura: u32,
    pub altura: u32,
    pub ficha: Option<Ficha>,
    /// A imagem foi feita no Revela (tem a ficha completa).
    pub do_revela: bool,
}

const EXTENSOES: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// Lista as imagens da pasta output (e das subpastas, um nível), as mais novas primeiro.
pub fn listar(output: &Path, limite: usize) -> Vec<Item> {
    let mut arquivos: Vec<(f64, PathBuf)> = Vec::new();
    let juntar = |dir: &Path, arquivos: &mut Vec<(f64, PathBuf)>, subpastas: &mut Vec<PathBuf>| {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let nome = e.file_name().to_string_lossy().to_string();
            if nome.starts_with('.') {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                subpastas.push(p);
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| EXTENSOES.contains(&x.to_lowercase().as_str())) {
                let t = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0);
                arquivos.push((t, p));
            }
        }
    };
    let mut subpastas = Vec::new();
    juntar(output, &mut arquivos, &mut subpastas);
    for s in subpastas {
        juntar(&s, &mut arquivos, &mut Vec::new());
    }
    arquivos.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    arquivos.truncate(limite);
    arquivos.into_iter().map(|(t, p)| item(&p, t)).collect()
}

/// Lê um item (tamanho e ficha) a partir do arquivo.
pub fn item(p: &Path, modificado: f64) -> Item {
    let nome = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut it = Item { caminho: p.to_string_lossy().into(), nome, modificado, largura: 0, altura: 0, ficha: None, do_revela: false };
    if let Ok(info) = crate::png::ler(p) {
        it.largura = info.largura;
        it.altura = info.altura;
        if let Some(f) = info.textos.get("revela").and_then(|t| serde_json::from_str::<Ficha>(t).ok()) {
            it.ficha = Some(f);
            it.do_revela = true;
        } else if let Some(v) = info.textos.get("prompt").and_then(|t| serde_json::from_str::<Value>(t).ok()) {
            it.ficha = ficha_do_comfy(&v, info.largura, info.altura);
        }
    }
    it
}

/// Ficha aproximada a partir do workflow gravado pelo ComfyUI (seed, texto e qualidade pelos passos).
pub fn ficha_do_comfy(fluxo: &Value, largura: u32, altura: u32) -> Option<Ficha> {
    let nos = fluxo.as_object()?;
    let (mut seed, mut prompt, mut passos) = (None, None, None);
    for no in nos.values() {
        let e = &no["inputs"];
        match no["class_type"].as_str().unwrap_or("") {
            "RandomNoise" => seed = seed.or(e["noise_seed"].as_u64()),
            "KSampler" | "KSamplerAdvanced" => seed = seed.or(e["seed"].as_u64().or(e["noise_seed"].as_u64())),
            "CLIPTextEncode" => {
                if let Some(t) = e["text"].as_str().filter(|t| !t.trim().is_empty()) {
                    prompt = prompt.or(Some(t.to_string()));
                }
            }
            "Flux2Scheduler" => passos = passos.or(e["steps"].as_u64()),
            _ => {}
        }
    }
    let qualidade = if passos.unwrap_or(20) <= 8 { Qualidade::Rapida } else { Qualidade::Caprichada };
    Some(Ficha {
        pedido: String::new(),
        prompt: prompt.unwrap_or_default(),
        termos: vec![],
        modo: String::new(),
        qualidade,
        seed: seed?,
        largura,
        altura,
        rascunho: false,
        referencias: vec![],
        versao: String::new(),
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::png::testes::png_falso;

    #[test]
    fn lista_e_le_fichas() {
        let dir = std::env::temp_dir().join(crate::id_unico());
        std::fs::create_dir_all(dir.join("Revela")).unwrap();
        // o ComfyUI grava o JSON em ASCII (json.dumps), com os acentos escapados
        let ficha = r#"{"pedido":"um ouri\u00e7o","prompt":"a hedgehog","qualidade":"rapida","seed":7,"largura":1024,"altura":1024}"#;
        std::fs::write(dir.join("Revela/REVELA_00001_.png"), png_falso(1024, 1024, &[("revela", ficha)])).unwrap();
        let comfy = r#"{"9":{"class_type":"SaveImage","inputs":{}},"75:73":{"class_type":"RandomNoise","inputs":{"noise_seed":432}},"75:74":{"class_type":"CLIPTextEncode","inputs":{"text":"party hat"}},"75:62":{"class_type":"Flux2Scheduler","inputs":{"steps":20}}}"#;
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("Flux2-Klein_00001_.png"), png_falso(1024, 1024, &[("prompt", comfy)])).unwrap();
        std::fs::write(dir.join("notas.txt"), "x").unwrap();

        let itens = listar(&dir, 10);
        assert_eq!(itens.len(), 2);
        assert_eq!(itens[0].nome, "Flux2-Klein_00001_.png"); // o mais novo primeiro
        let f0 = itens[0].ficha.as_ref().unwrap();
        assert_eq!((f0.seed, f0.prompt.as_str(), f0.qualidade), (432, "party hat", Qualidade::Caprichada));
        assert!(!itens[0].do_revela);
        let f1 = itens[1].ficha.as_ref().unwrap();
        assert_eq!((f1.seed, f1.pedido.as_str()), (7, "um ouriço"));
        assert!(itens[1].do_revela);
        std::fs::remove_dir_all(dir).ok();
    }
}
