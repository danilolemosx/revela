//! Workflows do ComfyUI em formato API (o mesmo JSON do "Export (API)"), montados a partir dos templates
//! "FLUX.2 [Klein] 4B: Text to Image" e "FLUX.2 [Klein] 4B: Image Edit". O app só troca o texto, as fotos,
//! o tamanho e o seed.
//!
//! - Rápida: modelo distilled, 4 passos, CFG 1, negativo zerado (ConditioningZeroOut).
//! - Caprichada: modelo base, 20 passos, CFG 5, negativo com texto vazio.
//! - Editar: cada foto é reduzida a ~1 MP, vira latente (VAEEncode) e entra como ReferenceLatent no positivo e no
//!   negativo, na ordem (foto 1, foto 2).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Qualidade {
    Rapida,
    Caprichada,
}

impl Qualidade {
    pub fn passos(self) -> u32 {
        match self {
            Qualidade::Rapida => 4,
            Qualidade::Caprichada => 20,
        }
    }
    pub fn cfg(self) -> f64 {
        match self {
            Qualidade::Rapida => 1.0,
            Qualidade::Caprichada => 5.0,
        }
    }
}

/// Nomes dos arquivos de modelo, como o ComfyUI os lista.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Modelos {
    pub rapida: String,
    pub caprichada: String,
    pub texto: String,
    pub vae: String,
}

impl Default for Modelos {
    fn default() -> Self {
        Modelos {
            rapida: "flux-2-klein-4b.safetensors".into(),
            caprichada: "flux-2-klein-base-4b.safetensors".into(),
            texto: "qwen_3_4b.safetensors".into(),
            vae: "flux2-vae.safetensors".into(),
        }
    }
}

/// Um pedido já em inglês, com as referências já enviadas ao ComfyUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pedido {
    pub prompt: String,
    /// Nomes no input do ComfyUI (o que o /upload/image devolve), de 0 a 2.
    #[serde(default)]
    pub referencias: Vec<String>,
    pub largura: u32,
    pub altura: u32,
    pub qualidade: Qualidade,
    pub seed: u64,
    /// Prefixo do arquivo no output (pode ter subpasta).
    #[serde(default = "prefixo_padrao")]
    pub prefixo: String,
}

pub fn prefixo_padrao() -> String {
    "Revela/REVELA".into()
}

/// Etapas do workflow, para a interface dizer o que está acontecendo.
pub fn etapa(no: &str) -> &'static str {
    match no {
        "texto" | "positivo" | "negativo" => "lendo o pedido",
        "unet" => "carregando o modelo",
        "vae" => "carregando o VAE",
        n if n.starts_with("ref") => "lendo as fotos",
        "gerar" => "gerando",
        "revelar" => "revelando a imagem",
        "salvar" => "salvando",
        _ => "preparando",
    }
}

/// Chave da medição de tempo: qualidade e quantas fotos de referência.
pub fn chave_tempo(q: Qualidade, fotos: usize) -> String {
    format!("{}-{}", if q == Qualidade::Rapida { "rapida" } else { "caprichada" }, fotos.min(2))
}

/// Segundos por passo por megapixel, medidos no Mac Studio de 32 GB (valem até o app medir no Mac de cada um).
pub fn seg_passo_padrao(q: Qualidade, fotos: usize) -> f64 {
    let base = [11.5, 25.0, 38.0][fotos.min(2)];
    match q {
        Qualidade::Rapida => base,
        Qualidade::Caprichada => base * 2.2, // CFG 5: duas passadas por passo
    }
}

/// Estimativa em segundos (sem o tempo de carregar os modelos na primeira vez).
pub fn estimar(q: Qualidade, fotos: usize, largura: u32, altura: u32, seg_por_mp: Option<f64>) -> f64 {
    let mp = (largura as f64 * altura as f64) / PIXELS_PADRAO;
    q.passos() as f64 * seg_por_mp.unwrap_or_else(|| seg_passo_padrao(q, fotos)) * mp + 6.0
}

/// Arredonda para um múltiplo de 16 (o FLUX.2 trabalha em blocos de 16 pixels).
pub fn mult16(v: f64) -> u32 {
    ((v / 16.0).round() as u32).max(1) * 16
}

/// Área padrão de saída (a de 1024×1024).
pub const PIXELS_PADRAO: f64 = 1024.0 * 1024.0;

/// Tamanho de saída com a proporção da foto e cerca de `pixels` de área (o "igual à foto").
pub fn tamanho_igual(largura: u32, altura: u32, pixels: f64) -> (u32, u32) {
    let (l, a) = (largura.max(1) as f64, altura.max(1) as f64);
    let escala = (pixels / (l * a)).sqrt();
    (mult16(l * escala), mult16(a * escala))
}

/// Rascunho rápido: metade da largura e da altura (um quarto dos pixels).
pub fn rascunho(largura: u32, altura: u32) -> (u32, u32) {
    (mult16(largura as f64 / 2.0), mult16(altura as f64 / 2.0))
}

fn liga(no: &str, saida: u32) -> Value {
    json!([no, saida])
}

fn no(fluxo: &mut Map<String, Value>, id: &str, classe: &str, entradas: Value) {
    fluxo.insert(id.into(), json!({ "class_type": classe, "inputs": entradas }));
}

/// Monta o workflow em formato API.
pub fn montar(p: &Pedido, m: &Modelos) -> Value {
    let mut f = Map::new();
    let q = p.qualidade;
    let unet = match q {
        Qualidade::Rapida => &m.rapida,
        Qualidade::Caprichada => &m.caprichada,
    };
    no(&mut f, "unet", "UNETLoader", json!({ "unet_name": unet, "weight_dtype": "default" }));
    no(&mut f, "texto", "CLIPLoader", json!({ "clip_name": m.texto, "type": "flux2", "device": "default" }));
    no(&mut f, "vae", "VAELoader", json!({ "vae_name": m.vae }));
    no(&mut f, "positivo", "CLIPTextEncode", json!({ "text": p.prompt, "clip": liga("texto", 0) }));
    match q {
        Qualidade::Rapida => no(&mut f, "negativo", "ConditioningZeroOut", json!({ "conditioning": liga("positivo", 0) })),
        Qualidade::Caprichada => no(&mut f, "negativo", "CLIPTextEncode", json!({ "text": "", "clip": liga("texto", 0) })),
    }

    // fotos de referência, na ordem: cada uma entra no positivo e no negativo
    let (mut pos, mut neg) = ("positivo".to_string(), "negativo".to_string());
    for (i, nome) in p.referencias.iter().take(2).enumerate() {
        let n = i + 1;
        let (abrir, escala, latente) = (format!("ref{n}_abrir"), format!("ref{n}_escala"), format!("ref{n}_latente"));
        no(&mut f, &abrir, "LoadImage", json!({ "image": nome }));
        no(
            &mut f,
            &escala,
            "ImageScaleToTotalPixels",
            json!({ "image": liga(&abrir, 0), "upscale_method": "nearest-exact", "megapixels": 1.0, "resolution_steps": 1 }),
        );
        no(&mut f, &latente, "VAEEncode", json!({ "pixels": liga(&escala, 0), "vae": liga("vae", 0) }));
        let (np, nn) = (format!("ref{n}_positivo"), format!("ref{n}_negativo"));
        no(&mut f, &np, "ReferenceLatent", json!({ "conditioning": liga(&pos, 0), "latent": liga(&latente, 0) }));
        no(&mut f, &nn, "ReferenceLatent", json!({ "conditioning": liga(&neg, 0), "latent": liga(&latente, 0) }));
        pos = np;
        neg = nn;
    }

    let (l, a) = (mult16(p.largura as f64), mult16(p.altura as f64));
    no(&mut f, "latente", "EmptyFlux2LatentImage", json!({ "width": l, "height": a, "batch_size": 1 }));
    no(&mut f, "passos", "Flux2Scheduler", json!({ "steps": q.passos(), "width": l, "height": a }));
    no(
        &mut f,
        "guia",
        "CFGGuider",
        json!({ "model": liga("unet", 0), "positive": liga(&pos, 0), "negative": liga(&neg, 0), "cfg": q.cfg() }),
    );
    no(&mut f, "amostrador", "KSamplerSelect", json!({ "sampler_name": "euler" }));
    no(&mut f, "ruido", "RandomNoise", json!({ "noise_seed": p.seed }));
    no(
        &mut f,
        "gerar",
        "SamplerCustomAdvanced",
        json!({
            "noise": liga("ruido", 0), "guider": liga("guia", 0), "sampler": liga("amostrador", 0),
            "sigmas": liga("passos", 0), "latent_image": liga("latente", 0)
        }),
    );
    no(&mut f, "revelar", "VAEDecode", json!({ "samples": liga("gerar", 0), "vae": liga("vae", 0) }));
    no(&mut f, "salvar", "SaveImage", json!({ "images": liga("revelar", 0), "filename_prefix": p.prefixo }));
    Value::Object(f)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pedido(refs: usize, q: Qualidade) -> Pedido {
        Pedido {
            prompt: "a hedgehog".into(),
            referencias: (0..refs).map(|i| format!("revela/ref{i}.png")).collect(),
            largura: 1344,
            altura: 768,
            qualidade: q,
            seed: 42,
            prefixo: prefixo_padrao(),
        }
    }

    /// Toda ligação aponta para um nó que existe.
    fn ligacoes_validas(f: &Value) {
        let mapa = f.as_object().unwrap();
        for (id, no) in mapa {
            for (k, v) in no["inputs"].as_object().unwrap() {
                if let Some(a) = v.as_array() {
                    let alvo = a[0].as_str().unwrap();
                    assert!(mapa.contains_key(alvo), "{id}.{k} aponta para {alvo}, que não existe");
                }
            }
        }
    }

    #[test]
    fn criar_rapida() {
        let f = montar(&pedido(0, Qualidade::Rapida), &Modelos::default());
        ligacoes_validas(&f);
        assert_eq!(f["unet"]["inputs"]["unet_name"], "flux-2-klein-4b.safetensors");
        assert_eq!(f["passos"]["inputs"]["steps"], 4);
        assert_eq!(f["guia"]["inputs"]["cfg"], 1.0);
        assert_eq!(f["negativo"]["class_type"], "ConditioningZeroOut");
        assert_eq!(f["latente"]["inputs"]["width"], 1344);
        assert_eq!(f["ruido"]["inputs"]["noise_seed"], 42);
        assert!(f.get("ref1_abrir").is_none());
    }

    #[test]
    fn editar_duas_fotos_caprichada() {
        let f = montar(&pedido(2, Qualidade::Caprichada), &Modelos::default());
        ligacoes_validas(&f);
        assert_eq!(f["unet"]["inputs"]["unet_name"], "flux-2-klein-base-4b.safetensors");
        assert_eq!(f["passos"]["inputs"]["steps"], 20);
        assert_eq!(f["negativo"]["class_type"], "CLIPTextEncode");
        // a foto 2 entra depois da foto 1, no positivo e no negativo
        assert_eq!(f["ref2_positivo"]["inputs"]["conditioning"], json!(["ref1_positivo", 0]));
        assert_eq!(f["ref2_negativo"]["inputs"]["conditioning"], json!(["ref1_negativo", 0]));
        assert_eq!(f["guia"]["inputs"]["positive"], json!(["ref2_positivo", 0]));
        assert_eq!(f["guia"]["inputs"]["negative"], json!(["ref2_negativo", 0]));
        assert_eq!(f["ref1_abrir"]["inputs"]["image"], "revela/ref0.png");
    }

    #[test]
    fn estimativas() {
        assert_eq!(chave_tempo(Qualidade::Caprichada, 3), "caprichada-2");
        let r = estimar(Qualidade::Rapida, 0, 1024, 1024, None);
        assert!((r - 52.0).abs() < 0.1, "{r}");
        assert!(estimar(Qualidade::Caprichada, 0, 1024, 1024, None) > 8.0 * 60.0);
    }

    #[test]
    fn tamanhos() {
        assert_eq!(tamanho_igual(1920, 1080, PIXELS_PADRAO), (1360, 768));
        assert_eq!(tamanho_igual(1024, 1024, PIXELS_PADRAO), (1024, 1024));
        assert_eq!(tamanho_igual(3000, 4000, PIXELS_PADRAO), (880, 1184));
        assert_eq!(rascunho(1344, 768), (672, 384));
        assert_eq!(mult16(1000.0), 1008);
    }
}
