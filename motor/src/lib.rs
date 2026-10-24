//! Motor do Revela. Toda a lógica fica aqui, sem nada de interface, para poder ser testada sozinha
//! (`cargo test -p motor`) e usada pela linha de comando (`motor-cli`).
//!
//! - `fluxo`: monta os workflows do ComfyUI em formato API (FLUX.2 Klein 4B, criar e editar).
//! - `comfy`: conversa com o ComfyUI local (/prompt, /upload/image, /history, /view, websocket).
//! - `local`: acha a instalação do ComfyUI Desktop e liga o motor escondido, em modo offline.
//! - `tradutor`: português → prompt em inglês com o Qwen3 4B no llama.cpp (llama-server local).
//! - `galeria`: lista os resultados da pasta output e lê a ficha gravada dentro do PNG.
//! - `imagem`: miniaturas e a cópia 1920×1080 para o Avid.

pub mod comfy;
pub mod fluxo;
pub mod galeria;
pub mod imagem;
pub mod local;
pub mod png;
pub mod tradutor;

/// Identificador curto e único (cliente do websocket, nomes de arquivo).
pub fn id_unico() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("revela-{:x}{:x}", n, std::process::id())
}

/// Hash FNV-1a de 64 bits (para dar nome estável às referências enviadas).
pub fn fnv64(dados: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in dados {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
