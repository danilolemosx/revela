//! Linha de comando do motor (para testar sem a interface).
//!
//!   motor-cli estado
//!   motor-cli ligar                                  liga o ComfyUI escondido (porta 8189) e fica esperando
//!   motor-cli fluxo criar|editar [--fotos N] [--caprichada]     imprime o workflow em formato API
//!   motor-cli traduzir "pedido" [--detalhar] [--fotos N]
//!   motor-cli gerar ("pedido" | --en "prompt") [--foto a.png]... [--tam 1344x768] [--caprichada] [--seed N] [--rascunho]
//!   motor-cli galeria [N]

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use motor::fluxo::{self, Pedido, Qualidade};
use motor::galeria::Ficha;
use motor::tradutor::{self, Modo, Tradutor};
use motor::{comfy, local};

fn valor(args: &[String], nome: &str) -> Option<String> {
    args.iter().position(|a| a == nome).and_then(|i| args.get(i + 1).cloned())
}

fn valores(args: &[String], nome: &str) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == nome).map(|w| w[1].clone()).collect()
}

fn tem(args: &[String], nome: &str) -> bool {
    args.iter().any(|a| a == nome)
}

fn tradutor_do(c: &local::Config) -> Result<Tradutor> {
    let bin = Tradutor::achar_binario().ok_or_else(|| anyhow!("não achei o llama-server"))?;
    Ok(Tradutor::new(bin, c.tradutor_modelo.clone(), tradutor::PORTA_PADRAO))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let resto = &args[1.min(args.len())..];
    match cmd {
        "estado" => {
            let c = local::carregar()?;
            println!("{}", serde_json::to_string_pretty(&c)?);
            let falta = local::modelos_faltando(&c);
            println!("modelos faltando: {}", if falta.is_empty() { "nenhum".into() } else { falta.join(", ") });
            match local::achar_motor(&c) {
                Some(m) => println!("ComfyUI no ar na porta {} · {}", m.porta, m.info()?["system"]["comfyui_version"]),
                None => println!("ComfyUI fechado"),
            }
            println!("llama-server: {:?}", Tradutor::achar_binario());
        }
        "ligar" => {
            let c = local::carregar()?;
            let log = local::pasta_app().join("comfyui.log");
            let t0 = Instant::now();
            let mut filho = local::ligar_comfy(&c, local::PORTA_REVELA, &log)?;
            println!("ComfyUI no ar na porta {} em {:.0} s (log em {})", local::PORTA_REVELA, t0.elapsed().as_secs_f64(), log.display());
            filho.wait()?;
        }
        "fluxo" => {
            let fotos: usize = valor(resto, "--fotos").and_then(|v| v.parse().ok()).unwrap_or(if resto.first().map(String::as_str) == Some("editar") { 1 } else { 0 });
            let p = Pedido {
                prompt: "PROMPT".into(),
                referencias: (1..=fotos).map(|i| format!("revela/FOTO{i}.png")).collect(),
                largura: 1024,
                altura: 1024,
                qualidade: if tem(resto, "--caprichada") { Qualidade::Caprichada } else { Qualidade::Rapida },
                seed: 0,
                prefixo: fluxo::prefixo_padrao(),
            };
            println!("{}", serde_json::to_string_pretty(&fluxo::montar(&p, &local::carregar().map(|c| c.modelos).unwrap_or_default()))?);
        }
        "traduzir" => {
            let c = local::carregar()?;
            let texto = resto.first().ok_or_else(|| anyhow!("falta o pedido"))?;
            let fotos = valor(resto, "--fotos").and_then(|v| v.parse().ok()).unwrap_or(0);
            let modo = if tem(resto, "--detalhar") { Modo::Detalhar } else { Modo::Traduzir };
            let mut t = tradutor_do(&c)?;
            let t0 = Instant::now();
            t.ligar(Some(&local::pasta_app().join("tradutor.log")))?;
            let t1 = Instant::now();
            let en = t.traduzir(texto, modo, fotos)?;
            println!("[abrir {:.1} s · traduzir {:.1} s]\n{en}", (t1 - t0).as_secs_f64(), t1.elapsed().as_secs_f64());
        }
        "gerar" => {
            let c = local::carregar()?;
            let m = local::achar_motor(&c).ok_or_else(|| anyhow!("o ComfyUI está fechado (rode `motor-cli ligar`)"))?;
            let fotos: Vec<PathBuf> = valores(resto, "--foto").into_iter().map(PathBuf::from).collect();
            let qualidade = if tem(resto, "--caprichada") { Qualidade::Caprichada } else { Qualidade::Rapida };
            let (pedido, prompt) = match valor(resto, "--en") {
                Some(en) => (String::new(), en),
                None => {
                    let pt = resto.first().filter(|a| !a.starts_with("--")).ok_or_else(|| anyhow!("falta o pedido"))?.clone();
                    let mut t = tradutor_do(&c)?;
                    let en = t.traduzir(&pt, Modo::Traduzir, fotos.len())?;
                    println!("inglês: {en}");
                    (pt, en)
                }
            };
            let (mut l, mut a) = match valor(resto, "--tam") {
                Some(t) => {
                    let (l, a) = t.split_once('x').ok_or_else(|| anyhow!("--tam é LARGURAxALTURA"))?;
                    (l.parse()?, a.parse()?)
                }
                None => match fotos.first() {
                    Some(f) => {
                        let i = motor::png::ler(f)?;
                        fluxo::tamanho_igual(i.largura, i.altura, fluxo::PIXELS_PADRAO)
                    }
                    None => (1024, 1024),
                },
            };
            if tem(resto, "--rascunho") {
                (l, a) = fluxo::rascunho(l, a);
            }
            let seed = valor(resto, "--seed").and_then(|s| s.parse().ok()).unwrap_or_else(|| motor::fnv64(motor::id_unico().as_bytes()) >> 14);
            let mut refs = Vec::new();
            for f in &fotos {
                refs.push(m.enviar_imagem(f)?);
            }
            let p = Pedido { prompt: prompt.clone(), referencias: refs, largura: l, altura: a, qualidade, seed, prefixo: fluxo::prefixo_padrao() };
            let ficha = Ficha {
                pedido,
                prompt,
                termos: vec![],
                modo: if fotos.is_empty() { "criar" } else { "editar" }.into(),
                qualidade,
                seed,
                largura: l,
                altura: a,
                rascunho: tem(resto, "--rascunho"),
                referencias: fotos.iter().map(|f| f.to_string_lossy().into()).collect(),
                versao: env!("CARGO_PKG_VERSION").into(),
            };
            println!("{}x{} · {:?} · seed {} · porta {}", l, a, qualidade, seed, m.porta);
            let fim = m.gerar(&fluxo::montar(&p, &c.modelos), Some(&serde_json::to_value(&ficha)?), &|| false, |e| match e {
                comfy::Evento::Previa(b) => eprint!("·{}", b.len() / 1024),
                comfy::Evento::Passo { valor, max, seg_passo, faltam_s } => {
                    println!("\npasso {valor}/{max} · {:?} s/passo · faltam {:?} s", seg_passo.map(|s| s.round()), faltam_s.map(|s| s.round()))
                }
                outro => println!("\n{}", serde_json::to_string(&outro).unwrap_or_default()),
            })?;
            match fim {
                comfy::Resultado::Pronto { saidas, tempo_s, .. } => {
                    for s in saidas {
                        println!("pronto em {tempo_s:.0} s: {}", s.caminho(&c.saida).display());
                    }
                }
                comfy::Resultado::Cancelado { .. } => println!("cancelado"),
            }
        }
        "galeria" => {
            let c = local::carregar()?;
            let n = resto.first().and_then(|v| v.parse().ok()).unwrap_or(10);
            for it in motor::galeria::listar(&c.saida, n) {
                let f = it.ficha.as_ref();
                println!("{} · {}x{} · seed {:?} · {}", it.nome, it.largura, it.altura, f.map(|f| f.seed), f.map(|f| f.prompt.chars().take(60).collect::<String>()).unwrap_or_default());
            }
        }
        _ => bail!("comandos: estado, ligar, fluxo, traduzir, gerar, galeria"),
    }
    Ok(())
}
