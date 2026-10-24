# Revela

Gerar e editar imagens com IA **100% no Mac**, para quem edita cinema e TV: plano que faltou, insert, tela de
celular, fundo, storyboard, referência para a direção de arte. Sem nuvem, sem conta e sem nada pago.

- **Criar do zero** ou **Editar foto** (1 ou 2 referências; o resultado pode virar a foto da próxima etapa).
- Escreva **em português**: um tradutor local (Qwen3 4B) escreve o prompt em inglês, e você vê o texto antes.
- Botões de linguagem de câmera (plano geral, plongée, lente 50 mm, película 35 mm…).
- Tamanhos 16:9, 9:16, 1:1, 2.39 e "igual à foto", rascunho rápido, qualidade rápida (~1 min) ou caprichada.
- Prévia ao vivo, tempo estimado, cancelar e fila.
- Galeria da pasta output com seed, pedido e etapas; "repetir com variação"; cópia 1920×1080 para o Avid.

## Como funciona
- Motor: o **ComfyUI** (GPL-3.0) da instalação do Comfy Desktop, pela API local (127.0.0.1). Se o Desktop estiver
  aberto, o Revela usa ele; senão liga o ComfyUI escondido, em modo `--offline`.
- Modelos: **FLUX.2 [klein] 4B** da Black Forest Labs (Apache 2.0), distilled (rápida) e base (caprichada).
- Tradutor: **Qwen3 4B Instruct** (Apache 2.0) no **llama.cpp** (MIT).
- Requisitos: Mac com chip Apple (M1 a M4), 32 GB de memória recomendados.

## Desenvolvimento

```bash
cargo test -p motor                                     # testes do motor
cargo run --release -p motor --bin motor-cli -- estado   # motor pela linha de comando
python3 testes-ui/simular.py                            # interface com dados simulados → testes-ui/capturas
cd app && cargo tauri build --bundles app               # o app
REVELA_AUTOTESTE=1 target/release/bundle/macos/Revela.app/Contents/MacOS/revela   # autoteste escondido
```

## Licenças
- Revela: MIT OR Apache-2.0.
- Fonte Archivo (Omnibus-Type): SIL Open Font License 1.1, veja `ui/fonts/OFL.txt`.
