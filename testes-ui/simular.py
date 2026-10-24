"""Monta a interface com um __TAURI__ falso e as imagens reais da pasta output, e tira capturas com o Chrome headless
(sem abrir janela). Uso: python3 testes-ui/simular.py [pasta_output]  → testes-ui/capturas/*.png e erros do console.
"""
import json
import os
import shutil
import struct
import subprocess
import sys

B = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
UI = os.path.join(B, 'ui')
SIM = os.path.join(B, 'testes-ui', 'sim')
CAP = os.path.join(B, 'testes-ui', 'capturas')
CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
OUTPUT = sys.argv[1] if len(sys.argv) > 1 else '/Volumes/INSIDE_10TB/02_SUITE APPS DL/07_IA IMAGENS/output'


def ler_png(p):
    """Tamanho e textos (tEXt) de um PNG."""
    info = {'l': 0, 'a': 0, 'textos': {}}
    with open(p, 'rb') as f:
        if f.read(8) != b'\x89PNG\r\n\x1a\n':
            return info
        while True:
            c = f.read(8)
            if len(c) < 8:
                break
            n, tipo = struct.unpack('>I', c[:4])[0], c[4:]
            if tipo in (b'IDAT', b'IEND'):
                break
            d = f.read(n + 4)[:n]
            if tipo == b'IHDR':
                info['l'], info['a'] = struct.unpack('>II', d[:8])
            elif tipo == b'tEXt' and b'\0' in d:
                k, v = d.split(b'\0', 1)
                info['textos'][k.decode('latin-1')] = v.decode('latin-1')
    return info


def galeria():
    itens = []
    for raiz, _, arqs in os.walk(OUTPUT):
        if raiz.count(os.sep) - OUTPUT.count(os.sep) > 1:
            continue
        for a in arqs:
            if not a.lower().endswith('.png'):
                continue
            p = os.path.join(raiz, a)
            i = ler_png(p)
            ficha = None
            if 'revela' in i['textos']:
                ficha = json.loads(i['textos']['revela'])
            itens.append({'caminho': p, 'nome': a, 'modificado': os.path.getmtime(p), 'largura': i['l'], 'altura': i['a'],
                          'ficha': ficha, 'doRevela': ficha is not None, 'mini': None, 'favorita': a.endswith('2_.png')})
    itens.sort(key=lambda x: -x['modificado'])
    return itens


GAL = galeria()
ESTADO = {'motor': 'ligado', 'porta': 8189, 'desktop': False, 'motorMsg': '', 'tradutor': 'ligado', 'tradutorMsg': '',
          'faltando': [], 'saida': OUTPUT, 'entrada': OUTPUT.replace('output', 'input'), 'modelos': OUTPUT.replace('output', 'models'),
          'comfyDir': '/Volumes/INSIDE_10TB/02_SUITE APPS DL/07_IA IMAGENS/danilolemos/ComfyUI',
          'log': '~/Library/Application Support/Revela/comfyui.log', 'segPasso': {'rapida-0': 11.2, 'rapida-1': 25.6}, 'favoritas': []}

FALSO = """
window.__TAURI__ = (() => {
  const ouvintes = {};
  window.__emitir = (n, p) => (ouvintes[n] || []).forEach((f) => f({ payload: p }));
  class Channel { constructor() { this.onmessage = null; window.__canal = this; } }
  const respostas = {
    estado: () => window.__estado,
    galeria: () => window.__galeria,
    dimensoes: (a) => { const g = window.__galeria.find((x) => x.caminho === a.caminho); return g ? [g.largura, g.altura] : [1344, 768]; },
    traduzir: (a) => new Promise((r) => setTimeout(() => r(window.__traducao || 'Place the hedgehog from image 1 on a bar counter at night, with blue and red neon lighting and rain streaming down the window behind.'), 150)),
    gerar: () => new Promise(() => {}),
    favoritar: () => null, cancelar: () => null, mostrar: () => null, ligar_motor: () => null, autoteste: () => false,
    para_avid: () => '/x/Para o Avid/REVELA_00003_1920x1080.png',
  };
  return {
    core: { invoke: async (cmd, a) => { console.log('invoke', cmd); const f = respostas[cmd]; if (!f) throw 'comando desconhecido ' + cmd; return f(a || {}); },
            convertFileSrc: (p) => 'file://' + encodeURI(p), Channel },
    event: { listen: async (n, f) => { (ouvintes[n] = ouvintes[n] || []).push(f); } },
    dialog: { open: async () => null },
    webview: { getCurrentWebview: () => ({ onDragDropEvent: () => {} }) },
  };
})();
"""


def pagina(nome, estado, roteiro=''):
    pasta = os.path.join(SIM, nome)
    shutil.rmtree(pasta, ignore_errors=True)
    shutil.copytree(UI, pasta)
    html = open(os.path.join(pasta, 'index.html')).read()
    dados = '<script>window.__estado=%s;window.__galeria=%s;</script>' % (json.dumps(estado), json.dumps(GAL))
    disparo = '<script>setTimeout(async()=>{%s},300)</script>' % roteiro
    html = html.replace('<script src="app.js"></script>', dados + '<script>%s</script><script src="app.js"></script>%s' % (FALSO, disparo))
    open(os.path.join(pasta, 'index.html'), 'w').write(html)
    png = os.path.join(CAP, nome + '.png')
    r = subprocess.run([CHROME, '--headless=new', '--disable-gpu', '--hide-scrollbars', '--allow-file-access-from-files',
                        '--user-data-dir=' + os.path.join(SIM, '.chrome'), '--virtual-time-budget=5000', '--window-size=1440,900',
                        '--force-device-scale-factor=1.5', '--enable-logging=stderr', '--v=0', '--screenshot=' + png,
                        'file://' + os.path.join(pasta, 'index.html')], capture_output=True, text=True)
    erros = [l for l in r.stderr.splitlines() if 'CONSOLE' in l and ('Error' in l or 'Uncaught' in l or 'error' in l)]
    print(png, '· erros:', len(erros))
    for e in erros[:8]:
        print('   ', e[-220:])


def esperar(ms):
    return 'await new Promise(r=>setTimeout(r,%d));' % ms


os.makedirs(CAP, exist_ok=True)
revela = [g for g in GAL if g['doRevela']]
r2 = next((g for g in GAL if g['nome'] == 'REVELA_00002_.png'), GAL[0])
r3 = next((g for g in GAL if g['nome'] == 'REVELA_00003_.png'), GAL[0])

# 1. abrindo: motor e tradutor ainda ligando
pagina('1-abrindo', dict(ESTADO, motor='ligando', tradutor='ligando', porta=None))

# 2. criar do zero, pedido traduzido, botões de câmera, resultado selecionado
pagina('2-criar', ESTADO, """
document.querySelector('#pt').value='Um ouriço com chapéu de festa no meio de confete, foto de câmera digital dos anos 2000, com flash';
window.__traducao='A hedgehog wearing a tiny party hat surrounded by confetti, early 2000s digital camera photo, flash photography.';
document.querySelector('#pt').dispatchEvent(new Event('input'));
document.querySelector('[data-c=medio]').click();
document.querySelector('[data-c=pelicula]').click();
%s
document.querySelector('[data-c="%s"]').click();
""" % (esperar(1300), r2['caminho']))

# 3. editar gerando: foto 1 da etapa anterior, prévia ao vivo, passo 2 de 4, dois na fila
pagina('3-editar-gerando', ESTADO, """
document.querySelector('[data-c="%s"]').click();
document.querySelector('#btUsar').click();
%s
document.querySelector('#pt').value='Coloque o ouriço da foto 1 em cima do balcão de um bar à noite, com luz de neon azul e vermelha e chuva escorrendo na janela atrás.';
document.querySelector('#pt').dispatchEvent(new Event('input'));
%s
document.querySelector('#gerar').click();
%s
document.querySelector('#btFila').click();
%s
document.querySelector('[data-t="916"]').click();
document.querySelector('#btFila').click();
%s
const img = await fetch('file://' + encodeURI('%s')).then(r=>r.blob()).catch(()=>null);
const c = window.__canal;
c.onmessage({tipo:'enviando'}); c.onmessage({tipo:'enfileirado', naFrente:0}); c.onmessage({tipo:'etapa', nome:'gerando'});
c.onmessage({tipo:'passo', valor:1, max:4}); c.onmessage({tipo:'passo', valor:2, max:4, segPasso:26, faltamS:52});
if (img) { const d = await new Promise((r)=>{ const f=new FileReader(); f.onload=()=>r(f.result.split(',')[1]); f.readAsDataURL(img); }); c.onmessage({tipo:'previa', dados:d}); }
""" % (r2['caminho'], esperar(300), esperar(1300), esperar(300), esperar(200), esperar(200), r3['caminho']))

# 4. etapas: resultado da edição selecionado, com a origem, e o menu do Avid aberto
pagina('4-etapas-avid', ESTADO, """
document.querySelector('[data-c="%s"]').click();
%s
document.querySelector('#btAvid').click();
""" % (r3['caminho'], esperar(200)))

# 5. ajustes
pagina('5-ajustes', ESTADO, "document.querySelector('#engr').click();")

# 6. sem ComfyUI Desktop
pagina('6-sem-comfy', dict(ESTADO, motor='ausente', tradutor='ausente', motorMsg='não achei o ComfyUI Desktop', saida=''))
