//! Leitura mínima de PNG: tamanho (IHDR) e textos (tEXt/iTXt), onde o ComfyUI grava o workflow e o Revela grava
//! a ficha (pedido em português, prompt em inglês, seed...).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Result};

const ASSINATURA: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Debug, Default, Clone)]
pub struct InfoPng {
    pub largura: u32,
    pub altura: u32,
    pub textos: BTreeMap<String, String>,
}

/// Lê só o começo do arquivo, até a imagem (os textos do ComfyUI ficam antes do IDAT).
pub fn ler(caminho: &Path) -> Result<InfoPng> {
    let mut f = std::fs::File::open(caminho)?;
    let mut cab = [0u8; 8];
    f.read_exact(&mut cab)?;
    if cab != ASSINATURA {
        bail!("não é PNG");
    }
    let mut info = InfoPng::default();
    loop {
        let mut c = [0u8; 8];
        if f.read_exact(&mut c).is_err() {
            break;
        }
        let n = u32::from_be_bytes([c[0], c[1], c[2], c[3]]) as usize;
        let tipo = [c[4], c[5], c[6], c[7]];
        if &tipo == b"IDAT" || &tipo == b"IEND" {
            break;
        }
        if n > 64 * 1024 * 1024 {
            bail!("pedaço grande demais no PNG");
        }
        let mut dados = vec![0u8; n + 4]; // + CRC
        f.read_exact(&mut dados)?;
        dados.truncate(n);
        match &tipo {
            b"IHDR" if n >= 8 => {
                info.largura = u32::from_be_bytes([dados[0], dados[1], dados[2], dados[3]]);
                info.altura = u32::from_be_bytes([dados[4], dados[5], dados[6], dados[7]]);
            }
            b"tEXt" => {
                if let Some(z) = dados.iter().position(|&b| b == 0) {
                    let chave = latin1(&dados[..z]);
                    info.textos.insert(chave, latin1(&dados[z + 1..]));
                }
            }
            b"iTXt" => {
                // chave \0 compressão(1) método(1) idioma \0 chave traduzida \0 texto (só o não comprimido)
                if let Some(z) = dados.iter().position(|&b| b == 0) {
                    let chave = latin1(&dados[..z]);
                    if dados.get(z + 1) == Some(&0) {
                        let resto = &dados[(z + 3).min(dados.len())..];
                        let mut partes = resto.splitn(3, |&b| b == 0);
                        let (_, _, texto) = (partes.next(), partes.next(), partes.next());
                        if let Some(t) = texto {
                            info.textos.insert(chave, String::from_utf8_lossy(t).into_owned());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(info)
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

#[cfg(test)]
pub(crate) mod testes {
    use super::*;

    fn pedaco(tipo: &[u8; 4], dados: &[u8]) -> Vec<u8> {
        let mut v = (dados.len() as u32).to_be_bytes().to_vec();
        v.extend(tipo);
        v.extend(dados);
        v.extend([0, 0, 0, 0]); // CRC não é conferido
        v
    }

    /// PNG falso só com cabeçalho e textos (o suficiente para o leitor).
    pub(crate) fn png_falso(l: u32, a: u32, textos: &[(&str, &str)]) -> Vec<u8> {
        let mut v = ASSINATURA.to_vec();
        let mut ihdr = l.to_be_bytes().to_vec();
        ihdr.extend(a.to_be_bytes());
        ihdr.extend([8, 2, 0, 0, 0]);
        v.extend(pedaco(b"IHDR", &ihdr));
        for (k, t) in textos {
            let mut d = k.as_bytes().to_vec();
            d.push(0);
            d.extend(t.as_bytes());
            v.extend(pedaco(b"tEXt", &d));
        }
        v.extend(pedaco(b"IDAT", &[0; 4]));
        v.extend(pedaco(b"IEND", &[]));
        v
    }

    #[test]
    fn le_tamanho_e_textos() {
        let dir = std::env::temp_dir().join(crate::id_unico());
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.png");
        std::fs::write(&p, png_falso(1344, 768, &[("revela", r#"{"seed":7}"#), ("prompt", "{}")])).unwrap();
        let i = ler(&p).unwrap();
        assert_eq!((i.largura, i.altura), (1344, 768));
        assert_eq!(i.textos["revela"], r#"{"seed":7}"#);
        std::fs::remove_dir_all(dir).ok();
    }
}
