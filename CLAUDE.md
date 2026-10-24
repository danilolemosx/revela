# Como trabalhar neste projeto

**Revela**: gerar e editar imagens com IA 100% local, para editores de cinema e TV (Corte Livre). Feito a partir do
modelo de app desktop do Danilo (Decanta). O processo completo está em `docs/processo.md`. Leia antes de começar.

## Como funciona
- Motor de imagem: o ComfyUI (GPL-3.0) da instalação do Comfy Desktop, pela API em 127.0.0.1. Se o Desktop estiver
  aberto (porta 8188), o app usa ele; senão liga o ComfyUI escondido na porta 8189 com `--offline` e sem o Manager
  (`motor/src/local.rs`). Configuração em `~/Library/Application Support/Revela/config.json`.
- Modelos (FLUX.2 Klein 4B, Apache 2.0) em `07_IA IMAGENS/models`: rápida = `flux-2-klein-4b` (4 passos, CFG 1),
  caprichada = `flux-2-klein-base-4b` (20 passos, CFG 5), `qwen_3_4b`, `flux2-vae`.
- Workflows em formato API montados em `motor/src/fluxo.rs` (cópias em `docs/workflows/`).
- Tradutor: Qwen3 4B Instruct Q4_K_M (`models/llm/`) no `llama-server` (Homebrew por enquanto), porta 8190.
  Regras e glossário em `motor/src/tradutor.rs`.
- A ficha (pedido, inglês, seed, tamanho) vai gravada dentro do PNG (texto "revela"); a galeria lê de lá.
- Tempos medidos no Mac Studio 32 GB: rápida 1024² ≈ 50 s com os modelos carregados (≈ 5 min na primeira, lendo do HD);
  editar com 1 foto 16:9 ≈ 2 min.

## Testes
- Motor: `cargo test -p motor` e
  `motor-cli estado | ligar | traduzir "..." [--detalhar] [--fotos N] | gerar "..." [--foto x.png] [--tam LxA] | galeria`
- Interface com dados simulados: `python3 testes-ui/simular.py` → `testes-ui/capturas/` (e os erros do console).
- App de verdade sem atrapalhar a tela: `REVELA_AUTOTESTE=1 target/release/bundle/macos/Revela.app/Contents/MacOS/revela
  -NSAppSleepDisabled YES` (janela escondida; cria, exporta para o Avid, edita, varia e cola; registro em
  `~/Library/Application Support/Revela/autoteste.log`; fecha sozinho).
- Lições: com a janela escondida o macOS congela os `setTimeout` da página (o autoteste só usa promessas e eventos);
  o `execution_success` do ComfyUI sai antes de o histórico ser gravado (o fim é o `executing` sem nó).

## Conversa
- Responder sempre em **português**, de forma **concisa**. O Danilo não é programador: explicar o que muda para ele, não o código.
- Trabalhar em **etapas com checkpoint**: ao fim de cada etapa, dizer o que ficou pronto e o que vem a seguir.
- **Antes de programar a interface, mostrar um preview** (imagem renderizada de um HTML de mockup) e iterar até ele aprovar.
  Mostrar a estética e também a operação: ele quer as duas coisas juntas, tudo numa tela só sempre que possível.
- Quando ele pedir "não faça ainda, apenas responda", só responder.
- Dar estimativa de tempo quando pedirem e oferecer o caminho mais simples quando houver trade-off.

## Código
- Toda a lógica no crate `motor/` (testável sozinho, com `motor-cli`); `app/` só liga a interface ao motor.
- Interface em HTML/CSS/JS puros em `ui/`, textos em português, fonte Archivo (`ui/fonts/`).
- Commits pequenos e descritivos, em português. Rodar `cargo test -p motor` antes de cada push.
- Se o app tiver de ser idêntico a um processo de referência, criar um teste automático que compare bit a bit (ver `docs/processo.md`).
- Testar o app de verdade antes de entregar: `xvfb-run` + captura de tela (Linux), e ver o resultado.

## Renomear o modelo para um app novo
Trocar "Modelo"/"modelo" em: `app/tauri.conf.json` (productName, mainBinaryName, identifier, title),
`app/Cargo.toml` (name, lib name), `app/src/main.rs` (modelo_lib), `.github/workflows/*.yml`, `ui/index.html`,
`README.md`. Gerar ícones: `cd app && cargo tauri icon icons/icon-fonte.png -o icons` (apagar `android/` e `ios/`).
