# Processo de criação de apps (aprendido no Decanta)

O Decanta (limpeza de diálogo com DeepFilterNet para o Avid) foi feito do zero até a v0.4 neste processo.
Repositório: github.com/danilolemosx/tratamento-audio-app.

## 1. Entender o pedido
- Anotar o que tem de ser **idêntico** a um processo existente e o que é livre.
- Perguntar só o que muda o caminho. Para o resto, escolher um padrão razoável e dizer qual foi.
- Dar a estimativa de tempo e as opções que economizam trabalho (ex.: fazer o Mac primeiro e o Windows depois).

## 2. Motor primeiro
- Implementar a lógica em `motor/` com a CLI e testes.
- Se houver uma referência (um binário, um script aprovado), criar um **teste que compara bit a bit** e rodá-lo
  no CI a cada envio. No Decanta isso pegou diferenças de 1 LSB que vinham da libm (glibc × musl): a solução foi
  embutir as funções matemáticas do musl.
- Fixar versões de dependências quando a fidelidade importar (partir do Cargo.lock da referência).

## 3. Interface: preview antes do código
1. Montar um **mockup em HTML**, renderizar como imagem (Playwright) e mandar.
2. Iterar: na primeira rodada ele aprovou a estética mas achou "muito cópia" da referência e com pouca operação.
   O que funcionou foi **mesclar estética e operação**: controles visíveis e óbvios, **tudo numa tela só**,
   pouca configuração.
3. Mostrar 2 ou 3 opções de fonte e paleta lado a lado quando o assunto for estética.
4. Antes de programar, perguntar "você faria alguma alteração?" e propor melhorias concretas e numeradas.
5. Só então programar e testar com dados simulados (Playwright) e com o app real (`xvfb-run`).

Preferências já conhecidas: estilo bilhete/ingresso (papel creme, tinta escura, vermelho), fonte larga e
geométrica (Archivo Expanded), números grandes, atalhos visíveis, nome de arquivo automático, prévia com antes e
depois. Ele prefere poucas opções e nada escondido, exceto o que é avançado (atrás de uma engrenagem).

## 4. CI e Release
- `testes.yml`: testes do motor em Linux, Mac e Windows a cada envio.
- `release.yml`: ao publicar uma Release com tag `v*`, o tauri-action gera os instaladores de Mac ARM, Mac Intel
  e Windows e os anexa à Release.
- Programas externos embutidos (ex.: bmx para MXF): compilar no CI para `app/binaries/<nome>-<alvo>` e
  declarar em `externalBin`.
- Depois do push, conferir o CI. Se for demorar, agendar uma checagem e avisar quando passar.

## 5. Distribuição
- O Danilo publica a Release pelo site (passo a passo no README) e baixa pelos links
  `https://github.com/<dono>/<repo>/releases/download/vX.Y.Z/<arquivo>`.
- Sem assinatura paga. Mac: `signingIdentity: "-"` (ad-hoc). Evita parte do erro "danificado", mas o
  Gatekeeper ainda pede "Abrir Mesmo Assim". Windows: o SmartScreen pede "Mais informações → Executar assim mesmo".
- Junto com os instaladores vai um LEIA-ME (modelo em `docs/LEIA-ME-modelo.txt`) e, para grupos, uma mensagem
  curta (modelo em `docs/mensagem-modelo.txt`).

## Erros já resolvidos (não repetir)
- **Mac "danificado"** ao baixar: assinar ad-hoc e ensinar `xattr -cr /Applications/<App>.app`.
- **"Não pode ser aberto" num Mac M1**: geralmente o .dmg errado (Intel × Apple Silicon) ou a quarentena.
- **Windows: MSVC sem `M_PI`** → `_USE_MATH_DEFINES`; símbolos duplicados com a ucrt ao embutir C → `_ACRTIMP=`.
- **JS travando ao abrir**: variável usada antes de ser criada (TDZ). Testar a interface com dados simulados no
  Playwright e olhar os erros do console.
- **Dependências antigas que não compilam** com o Rust novo: `cargo update -p <crate>`.
- **MXF do Avid**: só 48 kHz e quadros inteiros (completar com silêncio e alinhar o início ao quadro).
- **`cargo fmt` reformatou o projeto todo**: não rodar sem `rustfmt.toml`. Seguir o estilo existente.
