//! Imagens no disco: tamanho, miniaturas da galeria e a cópia 1920×1080 para o Avid.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, RgbImage};

pub const AVID_L: u32 = 1920;
pub const AVID_A: u32 = 1080;

/// Largura e altura sem abrir a imagem inteira.
pub fn dimensoes(p: &Path) -> Result<(u32, u32)> {
    image::image_dimensions(p).with_context(|| format!("não consegui ler {}", p.display()))
}

/// Miniatura JPEG (lado maior de 360 px) guardada em `cache`; só refaz se o arquivo mudou.
pub fn miniatura(origem: &Path, cache: &Path) -> Result<PathBuf> {
    let m = std::fs::metadata(origem)?;
    let quando = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let chave = crate::fnv64(format!("{}|{}|{}", origem.display(), quando, m.len()).as_bytes());
    let destino = cache.join(format!("{chave:016x}.jpg"));
    if destino.is_file() {
        return Ok(destino);
    }
    std::fs::create_dir_all(cache)?;
    let img = image::open(origem).with_context(|| format!("não consegui abrir {}", origem.display()))?;
    let mini = img.resize(360, 360, FilterType::Triangle).to_rgb8();
    mini.save_with_format(&destino, image::ImageFormat::Jpeg)?;
    Ok(destino)
}

/// Encaixa a imagem num quadro `l`×`a`: `preencher` corta as sobras (sem tarja); senão põe tarja preta.
pub fn enquadrar(img: &DynamicImage, l: u32, a: u32, preencher: bool) -> RgbImage {
    let (w, h) = img.dimensions();
    let escala = if preencher {
        (l as f64 / w as f64).max(a as f64 / h as f64)
    } else {
        (l as f64 / w as f64).min(a as f64 / h as f64)
    };
    let (nw, nh) = (((w as f64 * escala).round() as u32).max(1), ((h as f64 * escala).round() as u32).max(1));
    let redim = img.resize_exact(nw, nh, FilterType::Lanczos3).to_rgb8();
    let mut quadro = RgbImage::new(l, a);
    let (dx, dy) = (nw as i64 - l as i64, nh as i64 - a as i64);
    if preencher {
        // corta o centro
        let (x0, y0) = ((dx / 2).max(0) as u32, (dy / 2).max(0) as u32);
        for y in 0..a {
            for x in 0..l {
                quadro.put_pixel(x, y, *redim.get_pixel((x + x0).min(nw - 1), (y + y0).min(nh - 1)));
            }
        }
    } else {
        image::imageops::overlay(&mut quadro, &redim, -dx / 2, -dy / 2);
    }
    quadro
}

/// Grava a cópia 1920×1080 (PNG) em `pasta` e devolve o caminho.
pub fn para_avid(origem: &Path, pasta: &Path, preencher: bool) -> Result<PathBuf> {
    let img = image::open(origem).with_context(|| format!("não consegui abrir {}", origem.display()))?;
    std::fs::create_dir_all(pasta)?;
    let nome = origem.file_stem().map(|s| s.to_string_lossy().trim_end_matches('_').to_string()).unwrap_or_else(|| "imagem".into());
    let destino = pasta.join(format!("{nome}_{AVID_L}x{AVID_A}{}.png", if preencher { "" } else { "_tarja" }));
    enquadrar(&img, AVID_L, AVID_A, preencher).save_with_format(&destino, image::ImageFormat::Png)?;
    Ok(destino)
}

#[cfg(test)]
mod testes {
    use super::*;
    use image::Rgb;

    #[test]
    fn enquadra_sem_e_com_tarja() {
        // 1536×640 (2,39:1) num quadro 16:9: tarja em cima e embaixo
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(1536, 640, Rgb([200, 100, 50])));
        let tarja = enquadrar(&img, 1920, 1080, false);
        assert_eq!(tarja.dimensions(), (1920, 1080));
        assert_eq!(*tarja.get_pixel(960, 2), Rgb([0, 0, 0]));
        assert_eq!(*tarja.get_pixel(960, 540), Rgb([200, 100, 50]));
        let cheio = enquadrar(&img, 1920, 1080, true);
        assert_eq!(*cheio.get_pixel(960, 2), Rgb([200, 100, 50])); // sem tarja: corta as laterais
        assert_eq!(*cheio.get_pixel(0, 540), Rgb([200, 100, 50]));
        // 1344×768 é um pouco mais estreito que 16:9: a tarja fica nas laterais
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(1344, 768, Rgb([200, 100, 50])));
        assert_eq!(*enquadrar(&img, 1920, 1080, false).get_pixel(2, 540), Rgb([0, 0, 0]));
    }

    #[test]
    fn miniatura_e_avid_no_disco() {
        let dir = std::env::temp_dir().join(crate::id_unico());
        std::fs::create_dir_all(&dir).unwrap();
        let origem = dir.join("REVELA_00001_.png");
        RgbImage::from_pixel(1024, 1024, Rgb([10, 20, 30])).save(&origem).unwrap();
        let m = miniatura(&origem, &dir.join("mini")).unwrap();
        assert_eq!(dimensoes(&m).unwrap(), (360, 360));
        assert_eq!(miniatura(&origem, &dir.join("mini")).unwrap(), m); // reaproveita
        let a = para_avid(&origem, &dir.join("Para o Avid"), false).unwrap();
        assert_eq!(a.file_name().unwrap(), "REVELA_00001_1920x1080_tarja.png");
        assert_eq!(dimensoes(&a).unwrap(), (1920, 1080));
        std::fs::remove_dir_all(dir).ok();
    }
}
