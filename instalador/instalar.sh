#!/bin/zsh
# Instalação do motor do Revela na primeira abertura (Mac com chip Apple).
# Uso: instalar.sh <pasta do instalador> <pasta de destino> [completo|tradutor]
#   completo: Python, ComfyUI v0.42.5, modelos FLUX.2 Klein 4B e o tradutor (uns 28 GB)
#   tradutor: só o llama.cpp e o Qwen3 4B (2,5 GB), para quem já tem o ComfyUI Desktop com os modelos
# Escreve no stdout linhas que o app lê:  PASSO n|total|texto   PCT n   ERRO texto   PRONTO
# Pode ser rodado de novo: o que já foi baixado é aproveitado (downloads retomam de onde pararam).
# Só baixa de fontes oficiais (GitHub do uv, do ComfyUI e do llama.cpp; Hugging Face) e nada é enviado.
set -u
REC="$1"; DEST="$2"; MODO="${3:-completo}"
VERSAO=1
UV_VERSAO=0.13.4
COMFY_TAG=v0.42.5
LLAMA_BUILD=b11669
LLAMA_SHA256=dbed55493d72bc1ba2babc8770d669c2289b5bc4ee9fb2a4dfc158d45b7d13fd
KLEIN=https://huggingface.co/Comfy-Org/Flux-2-Klein-4B/resolve/533c4cbc3d3b89af1743d9466c434e89810391c0/split_files
QWEN=https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/a06e946bb6b655725eafa393f4a9745d460374c9

# pasta|url|tamanho em bytes
MODELOS=(
  "diffusion_models/flux-2-klein-4b.safetensors|$KLEIN/diffusion_models/flux-2-klein-4b.safetensors|7751105712"
  "diffusion_models/flux-2-klein-base-4b.safetensors|$KLEIN/diffusion_models/flux-2-klein-base-4b.safetensors|7751105712"
  "text_encoders/qwen_3_4b.safetensors|$KLEIN/text_encoders/qwen_3_4b.safetensors|8044982048"
  "vae/flux2-vae.safetensors|$KLEIN/vae/flux2-vae.safetensors|336211292"
)
TRADUTOR="llm/Qwen3-4B-Instruct-2507-Q4_K_M.gguf|$QWEN/Qwen3-4B-Instruct-2507-Q4_K_M.gguf|2497281120"
# teste do instalador sem os modelos grandes (só o VAE, de 336 MB, e o tradutor)
[[ -n "${REVELA_SEM_MODELOS:-}" ]] && MODELOS=("${MODELOS[4]}")

mkdir -p "$DEST/bin" "$DEST/models" "$DEST/logs" "$DEST/input"
LOG="$DEST/logs/instalacao.log"
exec 2>>"$LOG"
echo "==== $(date) · $MODO ====" >&2
export UV_PYTHON_INSTALL_DIR="$DEST/python" UV_CACHE_DIR="$DEST/cache" UV_NO_PROGRESS=1 DO_NOT_TRACK=1
export PATH="$DEST/bin:/usr/bin:/bin:/usr/sbin:/sbin"

if [[ "$MODO" == tradutor ]]; then TOTAL=3; PRECISA=4; else TOTAL=6; PRECISA=35; fi
N=0
passo() { N=$((N + 1)); echo "PASSO $N|$TOTAL|$1"; }
falha() { echo "ERRO $1"; exit 1; }
baixar() { # url destino
  local t
  for t in 1 2 3 4 5; do
    curl -fL --retry 3 -C - -s -S -o "$2" "$1" >&2 && return 0
    sleep 3
  done
  return 1
}
tamanho() { stat -f %z "$1" 2>/dev/null || echo 0; }
# baixa uma lista "pasta|url|bytes" para models/, conferindo o tamanho; mostra PCT do total
baixar_modelos() {
  local total=0 item p u b
  for item in "$@"; do total=$((total + ${item##*|})); done
  ( local feito f
    while sleep 3; do
      feito=0
      for f in "$@"; do
        p="${f%%|*}"
        [[ -f "$DEST/models/$p" ]] && feito=$((feito + $(tamanho "$DEST/models/$p")))
        [[ -f "$DEST/models/$p.part" ]] && feito=$((feito + $(tamanho "$DEST/models/$p.part")))
      done
      echo "PCT $(( feito * 100 / total > 99 ? 99 : feito * 100 / total ))"
    done ) &
  local medidor=$!
  for item in "$@"; do
    p="${item%%|*}"; u="${${item#*|}%|*}"; b="${item##*|}"
    mkdir -p "$DEST/models/${p:h}"
    if [[ "$(tamanho "$DEST/models/$p")" != "$b" ]]; then
      baixar "$u" "$DEST/models/$p.part" || { kill $medidor 2>/dev/null; falha "Não consegui baixar ${p:t}. Confira a internet e clique em tentar de novo (o que já baixou é aproveitado)."; }
      [[ "$(tamanho "$DEST/models/$p.part")" == "$b" ]] || { kill $medidor 2>/dev/null; rm -f "$DEST/models/$p.part"; falha "O arquivo ${p:t} veio incompleto. Tente de novo."; }
      mv "$DEST/models/$p.part" "$DEST/models/$p"
    fi
  done
  kill $medidor 2>/dev/null
  echo "PCT 100"
}

[[ "$(uname -m)" == arm64 ]] || falha "O Revela precisa de um Mac com chip Apple (M1, M2, M3, M4)."
LIVRE=$(df -g "$DEST" | awk 'NR==2 {print $4}')
(( LIVRE >= PRECISA )) || falha "Pouco espaço no disco: precisa de uns $PRECISA GB livres e há ${LIVRE} GB."

instalar_tradutor() {
  passo "Baixando o tradutor (llama.cpp $LLAMA_BUILD)"
  if [[ ! -x "$DEST/llama/llama-server" ]]; then
    baixar "https://github.com/ggml-org/llama.cpp/releases/download/$LLAMA_BUILD/llama-$LLAMA_BUILD-bin-macos-arm64.tar.gz" "$DEST/llama.tar.gz" \
      || falha "Não consegui baixar o llama.cpp. Confira a internet."
    [[ "$(shasum -a 256 "$DEST/llama.tar.gz" | cut -d' ' -f1)" == "$LLAMA_SHA256" ]] || { rm -f "$DEST/llama.tar.gz"; falha "O llama.cpp baixado não confere. Tente de novo."; }
    rm -rf "$DEST/llama" "$DEST/llama-$LLAMA_BUILD"
    tar -xzf "$DEST/llama.tar.gz" -C "$DEST" >&2 && mv "$DEST/llama-$LLAMA_BUILD" "$DEST/llama" && rm -f "$DEST/llama.tar.gz"
  fi
  "$DEST/llama/llama-server" --version >&2 || falha "O llama.cpp não abriu neste Mac."
}

if [[ "$MODO" == tradutor ]]; then
  instalar_tradutor
  passo "Baixando o modelo do tradutor (2,5 GB)"
  baixar_modelos "$TRADUTOR"
else
  passo "Baixando o instalador de Python"
  if [[ ! -x "$DEST/bin/uv" ]]; then
    baixar "https://github.com/astral-sh/uv/releases/download/$UV_VERSAO/uv-aarch64-apple-darwin.tar.gz" "$DEST/uv.tar.gz" || falha "Não consegui baixar o uv. Confira a internet."
    tar -xzf "$DEST/uv.tar.gz" -C "$DEST" >&2 && mv "$DEST/uv-aarch64-apple-darwin/uv" "$DEST/bin/uv" && rm -rf "$DEST/uv.tar.gz" "$DEST/uv-aarch64-apple-darwin"
  fi
  [[ -x "$DEST/bin/uv" ]] || falha "O uv não ficou instalado."

  passo "Baixando o ComfyUI $COMFY_TAG"
  C="$DEST/ComfyUI"
  if [[ ! -f "$C/.tag" || "$(cat "$C/.tag")" != "$COMFY_TAG" ]]; then
    rm -rf "$C" "$DEST/tmp"; mkdir -p "$DEST/tmp"
    baixar "https://github.com/Comfy-Org/ComfyUI/archive/refs/tags/$COMFY_TAG.tar.gz" "$DEST/tmp/c.tar.gz" || falha "Não consegui baixar o ComfyUI."
    tar -xzf "$DEST/tmp/c.tar.gz" -C "$DEST/tmp" >&2 && rm "$DEST/tmp/c.tar.gz" && mv "$DEST/tmp/"* "$C" && rm -rf "$DEST/tmp"
    [[ -f "$C/main.py" ]] || falha "O ComfyUI baixado veio incompleto."
    echo "$COMFY_TAG" > "$C/.tag"
  fi

  passo "Instalando o Python e as bibliotecas (uns 900 MB)"
  if [[ ! -x "$DEST/env/bin/python" ]]; then
    "$DEST/bin/uv" venv -q --python 3.13 "$DEST/env" >&2 || falha "Não consegui criar o ambiente Python."
  fi
  VIRTUAL_ENV="$DEST/env" "$DEST/bin/uv" pip install -q -r "$REC/requisitos.txt" >&2 || falha "Não consegui instalar as bibliotecas. Veja logs/instalacao.log."
  rm -rf "$DEST/cache"

  instalar_tradutor

  passo "Baixando os modelos FLUX.2 Klein 4B e o tradutor (uns 26 GB; pode demorar)"
  baixar_modelos "${MODELOS[@]}" "$TRADUTOR"
  cat > "$DEST/extra_model_paths.yaml" <<EOF
# gerado pelo instalador do Revela
revela:
  base_path: '$DEST/models'
  diffusion_models: diffusion_models/
  text_encoders: text_encoders/
  vae: vae/
EOF
fi

passo "Conferindo"
[[ -s "$DEST/models/${TRADUTOR%%|*}" ]] || falha "Faltou o modelo do tradutor."
if [[ "$MODO" != tradutor ]]; then
  for item in "${MODELOS[@]}"; do
    [[ -s "$DEST/models/${item%%|*}" ]] || falha "Faltou o modelo ${${item%%|*}:t}."
  done
  "$DEST/env/bin/python" -c "
import torch, comfyui_frontend_package
assert torch.backends.mps.is_available(), 'o MPS (GPU do chip Apple) não está disponível'
" >&2 || falha "O motor não abriu depois da instalação. Veja logs/instalacao.log."
fi
echo "$VERSAO $MODO" > "$DEST/.instalado"
echo "PRONTO"
