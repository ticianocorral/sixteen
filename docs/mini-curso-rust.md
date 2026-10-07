# Mini curso de Rust com o SixteeN

Um curso de Rust para quem nunca escreveu uma linha da linguagem, usando o
próprio SixteeN como material de estudo. Cada aula explica um conceito
do zero e mostra onde ele aparece **de verdade** no código deste repositório —
nada de exemplos soltos sobre `foo` e `bar`.

Como usar este curso:

1. Leia uma aula por vez, na ordem (as aulas se apoiam nas anteriores).
2. Abra os arquivos citados no editor e confira o código por conta própria.
3. Faça os exercícios — eles são curtos e todos cabem no próprio projeto.
4. Trave na aula 7 (posse e empréstimo)? Normal. É o coração de Rust e todo
   mundo trava. Volte nela quantas vezes precisar.

---

## Aula 0 — O que é Rust, e por que o SixteeN usa ele

Rust é uma linguagem de programação **compilada**: você escreve o código, roda
o compilador (`rustc`, quase sempre através do `cargo`, o "faz-tudo" de Rust),
e ele gera um programa executável nativo — igual C ou C++. Não existe
interpretador rodando junto, como em Python ou JavaScript.

O que Rust tem de diferente é uma promessa ousada: **segurança sem sacrificar
velocidade**. O compilador recusa programas que poderiam dar problema de
memória (acessar memória que já foi liberada, por exemplo) *antes* de você
conseguir rodar o programa. Em C/C++ esses erros existem e explodem em
produção; em Rust, em geral, viram erro de compilação.

Por que isso importa num emulador de SNES:

- **Velocidade**: o emulador precisa rodar 60 quadros por segundo. Código de
  emulação lento = jogo engasgado. Rust compila para código de máquina tão
  rápido quanto C++.
- **Confiabilidade**: o SixteeN carrega ROMs, abre bibliotecas
  `.dylib`/`.dll` de terceiros (os "cores" libretro, como o snes9x) e mexe
  com ponteiros. É exatamente o tipo de código onde C++ morde. Rust obriga a
  tornar cada risco explícito no código.
- **Mensagens de erro que ensinam**: o compilador de Rust é famoso por explicar
  o erro, apontar a linha e sugerir a correção. Trate-o como um professor
  rigoroso, não como um inimigo.

Se você já programa em outra linguagem: o seu maior ajuste será a **aula 7**
(posse e empréstimo). O resto — variáveis, funções, estruturas de dados — é
familiar com roupagem nova.

---

## Aula 1 — Conhecendo o terreno: o projeto como um mapa

Antes de escrever código, entenda como um projeto Rust de verdade é organizado.
Abra a raiz do repositório. O arquivo mais importante é o `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/emulation",
    "crates/platform",
    "crates/domain",
    "crates/ra",
    "crates/ntsc",
    "crates/app",
]

[workspace.package]
version = "1.1.5"
edition = "2021"
```

Três conceitos novos aqui:

- **Cargo** é a ferramenta oficial de Rust: compila o projeto, roda os testes,
  baixa bibliotecas ("crates") de terceiros, gera a documentação. É o `npm` +
  `make` + `pip` de Rust, tudo em um.
- **Crate** é a unidade de compilação de Rust — um pacote, uma biblioteca ou
  um executável. O SixteeN é um **workspace**: um repositório que
  contém vários crates que se ajudam.
- **`Cargo.lock`** trava as versões exatas de todas as dependências, para todo
  mundo compilar com as mesmas versões.

Os crates do projeto, do mais "conceitual" ao mais "prático":

| Crate | Papel | Tamanho |
|---|---|---|
| `domain` | Regras do negócio: o que é uma ROM, como identificá-la, catálogo, cheats | pequeno |
| `emulation` | Fala com os cores libretro (snes9x etc.) carregados em tempo de execução | pequeno |
| `platform` | Janela, teclado, mouse, áudio, e o desenho do console/TV | médio |
| `ntsc` | O filtro que faz a imagem parecer uma TV de tubo dos anos 90 | minúsculo |
| `ra` | RetroAchievements: troféus nos jogos | minúsculo |
| `app` | Cola tudo e define os programas executáveis (`sixteen`, `selector`...) | grande |

Repare na ideia: cada crate tem **uma responsabilidade**. O `domain` nem sabe
que janela existe — o comentário no topo de `crates/domain/src/lib.rs` diz:
*"Domain layer: ROM identity, catalogue and No-Intro naming. **No SDL**."*
(SDL é a biblioteca gráfica; ou seja: nada de tela aqui.)

### Instalando e rodando

```bash
# instalar Rust (uma vez só)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# dentro da pasta do projeto:
cargo build            # compila tudo
cargo run -p sixteen-app --bin selector   # roda a estante de jogos
cargo test             # roda TODOS os testes do workspace
cargo check            # só confere se compila (rápido, sem gerar executável)
```

`cargo check` vai virar seu melhor amigo: é o jeito barato de perguntar "será
que compila?" enquanto escreve código.

> **Primeiro exercício (aquecimento):** rode `cargo test` na raiz do projeto.
> Deve aparecer uma lista de testes e um `test result: ok`. Você acabou de
> compilar e validar o emulador inteiro sem escrever nada — e já viu o formato
> de saída do Cargo, que vai acompanhar você pelo curso inteiro.

---

## Aula 2 — Variáveis, tipos e funções: Rust com picket fence

Vamos ao código de verdade. Abra `crates/emulation/src/core.rs` e procure
esta função (perto da linha 68):

```rust
impl PixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb1555 | PixelFormat::Rgb565 => 2,
            PixelFormat::Xrgb8888 => 4,
        }
    }
}
```

Ignore o `impl` e o `match` por um instante (aulas 4 e 5) e foque no
assinatura: `bytes_per_pixel(self) -> usize`. Em Rust:

- Variáveis e funções têm **tipos explícitos**, escritos depois de `:`
  (parâmetros) ou `->` (retorno). Aqui, `usize` é o tipo "número inteiro do
  tamanho natural da máquina" — usado para tamanhos e índices.
- O estilo é *snake_case*: funções `bytes_per_pixel`, variáveis `header_len`.
  O compilador avisa se você fugir do padrão.

Agora variáveis. De `crates/domain/src/rom.rs` (linha 61):

```rust
// A raw SNES ROM is a multiple of 32 KiB. A leading 512-byte copier
// header shows up as a 512-byte remainder.
let header_len = if bytes.len() % 1024 == 512 { 512 } else { 0 };
let rom = &bytes[header_len..];
```

- `let` cria uma variável. **E por padrão ela é imutável**: depois desse `let`,
  ninguém pode fazer `header_len = 0;`. Se quiser mutabilidade, você pede
  explicitamente: `let mut total = 0;`. Isso parece teimosia, mas evita meia
  classe de bugs (alguém mexendo num valor que não devia mudar).
- Repare que `if` está sendo usado como uma **expressão** — ele *produz um
  valor*, que vai para `header_len`. Em Rust, quase tudo é expressão: `if`,
  `match`, blocos `{ }`. Não existe operador ternário `?:` porque `if a { 1 } else { 2 }` já faz isso.
- `bytes.len()` — o ponto chama métodos como em quase toda linguagem moderna.
- `&bytes[header_len..]` é uma "fatia" (slice): os bytes a partir de
  `header_len` até o fim. O `&` será explicado na aula 7 — por agora, leia-o
  como "uma referência a", sem copiar os dados.

Tipos numéricos que você vai encontrar no projeto: `u8` (byte, 0–255 — os
pixels das ROMs são arrays de `u8`), `u32`, `usize`, `i16` (áudio), `f32`/`f64`
(flotantes). E `bool`, `char`, `String`/`&str` (texto — aula 6).

Funções vivem soltas ou dentro de `impl`. Uma função solta de `rom.rs`
(linha 118):

```rust
fn score(name: &Option<String>) -> usize {
    match name {
        None => 0,
        Some(s) => s.chars().filter(|c| c.is_ascii_alphanumeric()).count(),
    }
}
```

- Sem `pub` na frente = **privada**: só existe dentro deste arquivo. `pub`
  significa "pública, visível para fora". Rust tem privacidade real por
  padrão — nada de convenção de underscore.
- O último bloco de uma função, **sem ponto-e-vírgula**, é o valor retornado.
  `count()` sem `;` é o `return`. (Pode usar `return` também, mas o estilo
  idiomático devolve a última expressão.)

> **Exercício 2:** em `crates/domain/src/rom.rs`, adicione este método dentro
> do `impl RomId`:
>
> ```rust
> /// Tamanho da ROM em formato humano ("32.0 KiB", "1.5 MiB").
> pub fn human_size(&self) -> String {
>     if self.rom_len >= 1024 * 1024 {
>         format!("{:.1} MiB", self.rom_len as f64 / (1024.0 * 1024.0))
>     } else {
>         format!("{:.1} KiB", self.rom_len as f64 / 1024.0)
>     }
> }
> ```
>
> Rode `cargo check -p sixteen-domain`. Compilou? Você acabou de alterar o
> emulador. Detalhes novos: `format!` monta texto (o `{:.1}` é uma casa
> decimal), `as f64` converte tipo numérico, e `&self` no parâmetro significa
> "o próprio objeto" (aula 7). Para *ver* funcionando, a aula 9 te ensina a
> escrever o teste.

---

## Aula 3 — Structs e enums: modelando o mundo do console

### Struct: um punhado de dados com nome

Ainda em `crates/domain/src/rom.rs` (linha 12), o tipo que representa "a
identidade de uma ROM":

```rust
#[derive(Debug, Clone)]
pub struct RomId {
    /// Length of the copier header that was skipped (0 or 512).
    pub header_len: usize,
    /// Headerless payload length in bytes.
    pub rom_len: usize,
    pub crc32: String,
    pub sha1: String,
    /// 21-byte title from the SNES internal header, trimmed. Best-effort.
    pub internal_name: Option<String>,
    /// LoROM / HiROM guess, for diagnostics.
    pub mapper: Mapper,
}
```

Leitura linha a linha:

- `pub struct RomId { ... }` define o tipo. Cada campo tem nome, tipo, e o
  seu próprio `pub` (os comentários `///` são **documentação** — o
  `cargo doc` gera páginas HTML com eles, e o editor os mostra como dicas).
- `#[derive(Debug, Clone)]` é um **atributo**: pede ao compilador para gerar,
  automaticamente, código que permite imprimir o struct no terminal (`Debug`)
  e copiá-lo (`Clone`). Você escreve `#[derive(...)]` e economiza código
  entediante. Outros comuns: `PartialEq` (comparar com `==`), `Copy`,
  `Default`, `Hash`.

Constrói-se um struct assim (é literalmente o final de `RomId::from_bytes`):

```rust
Ok(RomId {
    header_len,
    rom_len: rom.len(),
    crc32,
    sha1,
    internal_name,
    mapper,
})
```

Detalhe elegante: quando a variável tem o mesmo nome do campo
(`header_len` para o campo `header_len`), não precisa repetir
`header_len: header_len`. Chama-se *field init shorthand*.

### Enum: um valor que é UM de vários

O mesmo arquivo, linha 26 — o tipo do "chip de mapeamento" do cartucho:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Mapper {
    LoRom,
    HiRom,
    Unknown,
}
```

Um valor `Mapper` é **exatamente uma** dessas três variantes — nunca duas,
nunca nenhuma. Não existe `Mapper` inválido. Compare com uma linguagem sem
enums ricos, onde você guardaria `mapper: int` e torceria para ninguém
escrever `17`: aqui o compilador não deixa.

Enums do projeto que você vai ver por toda parte, em
`crates/emulation/src/core.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Button {
    B = RETRO_DEVICE_ID_JOYPAD_B,
    Y = RETRO_DEVICE_ID_JOYPAD_Y,
    // ... todos os botões do controle de SNES
    L = RETRO_DEVICE_ID_JOYPAD_L,
    R = RETRO_DEVICE_ID_JOYPAD_R,
}

pub enum PixelFormat {
    Rgb1555,
    Xrgb8888,
    Rgb565,
}
```

Enums também podem **carregar dados dentro de cada variante** — aKill feature
de Rust, que usamos o tempo todo para erros (aula 5).

E métodos em enums, como na `bytes_per_pixel` da aula 2: `impl PixelFormat { ... }`.

> **Exercício 3:** adicione a variante `SuperFx` ao enum `Mapper` (era o chip
> dos jogos como *Star Fox*). Rode `cargo check` no workspace **inteiro**.
> Provavelmente nada quebra — mas se algum `match` sobre `Mapper` não cobrir
> a variante nova, o compilador aponta o arquivo e a linha exatos. É o
> momento "uau" de Rust: adicione um caso ao mundo e deixe o compilador te
> dizer tudo que precisa se adaptar.

---

## Aula 4 — `match`: o interruptor de trem de Rust

`match` pega um valor e escolhe o que fazer conforme ele é. É como o
`switch` de outras linguagens, mas **exaustivo**: se você esquecer um caso, o
código não compila.

A função `bytes_per_pixel` da aula 2 já era um `match`. Um mais
interessante, `crates/domain/src/rom.rs` linha 91 — decidir se a ROM é LoROM
ou HiROM comparando qual título interno parece mais com texto de verdade:

```rust
let lo = title_at(rom, LO);
let hi = title_at(rom, HI);
match (score(&lo), score(&hi)) {
    (l, h) if h > l => (Mapper::HiRom, hi),
    (l, _) if l > 0 => (Mapper::LoRom, lo),
    _ => (Mapper::Unknown, None),
}
```

Novidades, uma por linha:

- `match (a, b)` — dá match em uma **tupla** (um par de valores agrupados).
- `(l, h) if h > l => ...` — *padrão com guarda*: casa quando além do formato
  a condição `h > l` vale.
- `_ => ...` — o "qualquer outra coisa". Obrigatório se nem todo caso foi
  coberto (aqui, o empate 0×0).

Um outro, de `rom.rs` linha 118, com enums que carregam dados:

```rust
fn score(name: &Option<String>) -> usize {
    match name {
        None => 0,
        Some(s) => s.chars().filter(|c| c.is_ascii_alphanumeric()).count(),
    }
}
```

Aqui o enum é `Option` (aula 5) e o padrão `Some(s)` **desempacota** o valor
de dentro, batizando-o de `s`. Isso é *pattern matching*: os padrões não só
escolhem um ramo, eles extraem e nomeiam os dados de dentro.

E o `match` mais dramático do projeto, o coração da conversa com os cores —
`crates/emulation/src/core.rs` linha 561, que responde a centenas de
"comandos" que o core libretro manda:

```rust
unsafe extern "C" fn environment_cb(cmd: c_uint, data: *mut c_void) -> bool {
    match cmd {
        RETRO_ENVIRONMENT_GET_CAN_DUPE => { /* ... */ true }
        RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => { /* ... */ }
        RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => { /* ... */ true }
        // ... dezenas de casos ...
        _ => false,   // comando desconhecido: "não sei responder isso"
    }
}
```

> **Exercício 4:** escreva (num arquivo novo `src/exercicio.rs` dentro de
> `sixteen-domain`, ou num playground com `cargo new`) uma função:
>
> ```rust
> enum Regiao { Ntsc, Pal }
>
> fn fps(regiao: Regiao) -> f64 {
>     // NTSC rodava a 60 Hz; PAL, a 50 Hz.
>     match regiao {
>         Regiao::Ntsc => 60.0,
>         Regiao::Pal => 50.0,
>     }
> }
> ```
>
> Depois quebre de propósito: comente o caso `Pal`. Tente compilar. Leia a
> mensagem de erro com calma — ela é literalmente `non-exhaustive patterns`
> e mostra o caso faltando. Esse é o tipo de proteção que você vai sentir
> falta em outras linguagens.

---

## Aula 5 — `Option` e `Result`: o fim do `null` e das exceções surpresa

Rust **não tem `null` e não tem exceções**. No lugar, dois enums padrão que
você viu pedaços já:

```rust
enum Option<T> {
    Some(T),   // tem um valor, e ele é este
    None,      // não tem valor
}

enum Result<T, E> {
    Ok(T),     // deu certo, aqui está o valor
    Err(E),    // deu errado, aqui está o erro
}
```

A diferença conceitual: `Option` para "pode não existir" (o título interno da
ROM — nem toda ROM tem um válido), `Result` para "pode falhar" (ler um
arquivo do disco).

### O jeito Rust de dizer "pode falhar"

O construtor principal do projeto, `crates/domain/src/rom.rs` linha 55:

```rust
pub fn from_bytes(bytes: &[u8]) -> Result<Self, RomError> {
    if bytes.len() < 0x8000 {
        return Err(RomError::TooSmall(bytes.len()));
    }
    // ... calcula hashes, detecta header ...
    Ok(RomId { /* ... */ })
}
```

A assinatura **declara** a possibilidade de falha e **qual** o tipo de erro.
Quem chama não tem como ignorar: para chegar ao `RomId`, é obrigatório passar
pelo `Result`. E os erros também são enums com dados:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RomError {
    #[error("could not read ROM {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("file is too small to be a SNES ROM ({0} bytes)")]
    TooSmall(usize),
}
```

(O `thiserror` é uma biblioteca que gera o código de implementação do trait
`Error`; o `#[error("...")]` vira a mensagem bonita que o `.display()` mostra.)

### `?`: o operador que você vai mais usar

De `rom.rs` linha 46:

```rust
pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RomError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| RomError::Read {
        path: path.display().to_string(),
        source,
    })?;
    Self::from_bytes(&bytes)
}
```

O `?` no fim de `fs::read(...)?` quer dizer: *"se deu errado, retorne o erro
agora da minha função; se deu certo, me dê o valor e siga"*. É o caminho
feliz escrito como caminho feliz — sem pirâmide de `try/catch` nem checagens
manuais. `map_err` traduz o erro de um tipo para outro antes do `?`.

### Lidando com `Option` no dia a dia

Do projeto, três padrões que valem ouro (todos reais):

```rust
// 1. Valor padrão quando não existe (core.rs):
pub fn frame_duped(&self) -> bool { self.state.frame_duped }

// 2. Encadear buscas que podem falhar (core.rs, variável):
self.state.variables.iter()
    .find(|(ek, _)| *ek == k)          // Option<&(CString, CString)>
    .and_then(|(_, v)| v.to_str().ok()) // se achou, converte; pode virar None

// 3. Retirar o valor de dentro, deixando vazio (core.rs):
pub fn take_frame(&mut self) -> Option<Frame> {
    self.state.frame.take()   // pega o frame e deixa None no lugar
}
```

> **Exercício 5:** no seu arquivo de exercícios, escreva:
>
> ```rust
> fn titulo_da_rom(bytes: &[u8]) -> Option<String> {
>     // Dica: se bytes.len() < 0x7FC0 + 21, devolva None.
>     // Senão, pegue os 21 bytes em 0x7FC0, troque bytes não-imprimíveis
>     // por espaço, faça .trim() e devolva Some(s) se não ficou vazio.
>     todo!()  // todo!() faz compilar, mas panica ao rodar — substitua
> }
> ```
>
> Você está reimplementando (em versão simplificada) a função `title_at` de
> `rom.rs` linha 98. Depois de tentar, compare com a original — a sua não
> precisa ficar igual, precisa compilar e fazer sentido *para você*.

---

## Aula 6 — Coleções, iteradores e closures: o canivete

As três coleções que você vai encontrar no projeto:

```rust
Vec<u8>       // lista de bytes — os pixels de um frame, o conteúdo de uma ROM
Vec<String>   // lista de textos — extensões válidas do core
[String; 12]  // array de tamanho FIXO — Button::ALL, os 12 botões do pad
HashMap<K, V> // dicionário
```

`Vec` é o cavalo de batalha: cresce quando precisa. `Vec::new()`,
`vec![0u8; 0x8000]` (32 KiB de zeros — aparece nos testes de `rom.rs`),
`push`, `len`, `iter`.

### Iteradores: loops em alta definição

Você já viu este trecho na função `score`:

```rust
s.chars()                              // 1. um iterador de chars
 .filter(|c| c.is_ascii_alphanumeric()) // 2. fica só com letras/números
 .count()                               // 3. consome e conta
```

E este, do teste `strips_copier_header` (`rom.rs` linha 134):

```rust
for (i, b) in b"TEST ROM TITLE".iter().enumerate() {
    rom[0x7FC0 + i] = *b;
}
```

O modelo mental: um iterador é uma esteira. Você encaixa etapas (`filter`,
`map`, `take`...) e ela só roda quando alguém "consome" a esteira (`count`,
`collect`, `for`). Preguiçoso e sem alocações intermediárias — o `filter`
acima não criou lista nenhuma.

As `|c| ...` são **closures**: funções anônimas, como as arrow functions de
JavaScript ou as lambdas de Python. `|c| c.is_ascii_alphanumeric()` recebe um
`c` e devolve o resultado daquele método. Elas "capturam" variáveis de fora
quando precisam — e Rust te faz declarar *como* (por referência ou
dono), o que de novo é aula 7.

Um `collect` de verdade, do carregamento do core (`core.rs` linha 231):

```rust
let valid_extensions = cstr_to_string(info.valid_extensions)
    .split('|')                      // "sfc|smc|swc" -> ["sfc", "smc", "swc"]
    .filter(|s| !s.is_empty())
    .map(|s| s.to_ascii_lowercase()) // cada uma em minúsculas
    .collect::<Vec<String>>();       // esteira -> Vec de novo
```

> **Exercício 6:** dado `let jogos = vec!["Zelda", "mario world", "F-ZERO"];`,
> escreva uma única expressão com iteradores que devolva um `Vec<String>`
> com os nomes em maiúsculas (`to_uppercase()`) dos jogos cujo nome tem **mais
> de 5 caracteres** (`chars().count()`). Resposta esperada:
> `["MARIO WORLD", "F-ZERO"]` — "Zelda" fica de fora: tem exatamente 5, e a
> condição é *mais* que 5. Depois brinque: troque para `>= 5` e veja mudar
> só ali, na borda.

---

## Aula 7 — Posse e empréstimo: a aula que separa Rust do resto

Aqui está a ideia central de Rust. Vá com calma.

**Regra de ouro: todo valor tem exatamente um dono, e quando o dono sai de
cena, o valor é destruído.**

```rust
{
    let pixels = vec![0u8; 256 * 224 * 2];  // pixels é o DONO do buffer
    usar(&pixels);                           // usar pega emprestado
}   // <- fim do bloco: pixels sai de escopo, a memória é liberada. Ponto.
```

Não existe `free` nem `delete`: a destruição é automática e determinística
(no fim do escopo). É o que o comentário do projeto quer dizer em
`crates/emulation/src/core.rs` linha 77:

```rust
/// One video frame handed up by the core. Owns its pixels.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
    pub format: PixelFormat,
    pub pixels: Vec<u8>,   // o frame É dono dos pixels
}
```

O `Core` entrega um `Frame` para quem pede (`take_frame`), e esse Frame leva
os pixels dele — ninguém mais os controla. Quando o Frame morrer, os pixels
morrem com ele. Zero vazamento, zero use-after-free, garantido pelo
compilador.

E o `Drop` do `Core` (linha 542) — "quando eu morrer, avise o core libretro":

```rust
impl Drop for Core {
    fn drop(&mut self) {
        if self.loaded {
            self.enter(|api| unsafe { (api.retro_unload_game)() });
        }
        self.enter(|api| unsafe { (api.retro_deinit)() });
    }
}
```

`Drop` é o destrutor de Rust: roda na saída de escopo. O código do app nunca
chama `retro_deinit` "à mão" — basta o `Core` sair de cena.

**E os empréstimos?** Copiar dados toda hora seria caro (imagine copiar os
4 MB da ROM a cada função). Então você *empresta* via referências:

- `&T` — empréstimo **somente leitura**. Pode ter **quantos quiser** ao
  mesmo tempo.
- `&mut T` — empréstimo de **leitura e escrita**. Só pode existir **um** por
  vez, e nenhum `&` junto.

Essas duas regras juntas são o *borrow checker* — o sujeito que faz o Rust
compilar "mais devagar" no começo. Por que elas existem? Porque praticamente
todo bug de memória em C/C++ envolve alguém escrevendo num dado enquanto
outrem lê. Com as regras acima, essa classe de bug não compila.

Como isso aparece no código real do projeto? Olhe as assinaturas do `Core`:

```rust
pub fn system_name(&self) -> &str          // "me empresta o nome, só leitura"
pub fn audio(&self) -> &[i16]              // "me empresta o áudio"
pub fn run(&mut self)                      // "preciso me MODIFICAR para rodar"
pub fn set_button(&mut self, port: usize, button: Button, pressed: bool)
```

O `&self` / `&mut self` em cada método é o dono declarando os termos do
empréstimo. E o compilador garante que ninguém guarda o `&[i16]` de áudio
enquanto chama `run(&mut self)` — porque `run` limpa e recheia o áudio.
Um bug sutil de ponteiro pendente, impossível por construção.

Quando *mover* (transferir a posse) também aparece — e aparece mesmo — é no
retorno: `save_state(&mut self) -> Option<Vec<u8>>` **cria** o buffer de
bytes e **entrega a posse** para quem chamou. Ninguém empresta aquilo; agora
é do chamador.

> **Exercício 7 (só de leitura, prometo):** abra `crates/domain/src/rom.rs`
> linha 98 e leia `title_at` devagar:
>
> ```rust
> fn title_at(rom: &[u8], off: usize) -> Option<String> {
>     let end = off + 21;
>     if rom.len() < end {
>         return None;
>     }
>     let raw = &rom[off..end];   // EMPRÉSTIMO dos 21 bytes (não copia)
>     let s: String = raw.iter()
>         .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { ' ' })
>         .collect();             // aqui SIM copia: vira uma String dona
>     let s = s.trim().to_string();
>     (!s.is_empty()).then_some(s)
> }
> ```
>
> Aponte mentalmente: onde o código *pede emprestado*? (`&rom[off..end]`)
> Onde ele *torna-se dono*? (o `collect` cria uma `String` nova)
> Onde a posse é *entregue*? (o `return` da `String`)
> Se você respondeu essas três, entendeu a aula. O `then_some` no final é um
> helper: "se a condição for verdade, vire `Some(s)`; senão, `None`".

---

## Aula 8 — Módulos e crates: como o projeto se organiza

Dentro de um crate, o código se divide em **módulos** (arquivos e pastas).
Veja `crates/domain/src/lib.rs` inteiro:

```rust
//! Domain layer: ROM identity, catalogue and No-Intro naming. No SDL.

pub mod catalog;
pub mod cheats;
pub mod library;
pub mod nointro;
pub mod rom;
pub mod tosec;

pub use catalog::{Catalog, CatalogEntry, CatalogError, Order, RomRow};
pub use cheats::{for_title as cheats_for_title, CheatDef};
pub use library::{scan, ScannedRom, ROM_EXTS};
pub use nointro::{NoIntroDat, NoIntroError};
pub use rom::{Mapper, RomError, RomId};
pub use tosec::TosecInfo;
```

- `pub mod rom;` diz: "existe um módulo chamado `rom`, o código está em
  `src/rom.rs`". O arquivo `rom.rs` que estudamos é um desses.
- `pub use rom::RomId;` **reexporta**: deixa `RomId` disponível direto na
  raiz do crate. É por isso que outros crates escrevem
  `use sixteen_domain::RomId;` e não `use sixteen_domain::rom::RomId;`.
  O `lib.rs` funciona como a "vitrine" do crate: ele escolhe o que a rua vê.
- O `//!` no topo é comentário de documentação **do módulo** (o `///` era de
  item).

Entre crates, usa-se o nome do pacote. Em `crates/app/Cargo.toml`, o app
declara de quem precisa:

```toml
[dependencies]
sixteen-emulation = { path = "../emulation" }
sixteen-platform  = { path = "../platform" }
sixteen-domain    = { path = "../domain" }
anyhow.workspace    = true
serde.workspace     = true
```

Dois tipos de dependência: os crates **do próprio workspace** (por `path`) e
os de terceiros do registry crates.io (`anyhow`, `serde` — versionados no
`[workspace.dependencies]` do `Cargo.toml` raiz, para todos os crates
usarem a mesma versão).

Para usar no código, um `use`:

```rust
use sixteen_domain::{RomId, ROM_EXTS};
```

> **Exercício 8:** o crate `app` usa a identidade de ROM em
> `crates/app/src/runner.rs` linha 1089:
> `match sixteen_domain::RomId::from_bytes(&rom_bytes) { ... }`.
> Abra e leia. Depois responda: quando o app chama `RomId::from_bytes`,
> qual dos dois crates "sabe" como calcular SHA-1? (Dica: é o mesmo que tem
> a dependência do `sha1` no `Cargo.toml`.) Essa separação — quem *usa* a
> informação não é quem *sabe calcular* — é o motivo de o projeto estar
> fatiado assim.

---

## Aula 9 — Testes: o costume que Rust vem com de fábrica

Testes são cidadãos de primeira classe: vivem **no mesmo arquivo** do código,
num módulo especial. O fim de `crates/domain/src/rom.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;   // importa tudo do arquivo de cima

    /// A 32 KiB blob with a LoROM-looking title should hash headerless and,
    /// with 512 bytes prepended, produce the *same* hashes.
    #[test]
    fn strips_copier_header() {
        let mut rom = vec![0u8; 0x8000];
        for (i, b) in b"TEST ROM TITLE".iter().enumerate() {
            rom[0x7FC0 + i] = *b;
        }
        let bare = RomId::from_bytes(&rom).unwrap();
        assert_eq!(bare.header_len, 0);
        assert_eq!(bare.rom_len, 0x8000);

        let mut headered = vec![0u8; 512];
        headered.extend_from_slice(&rom);
        let hd = RomId::from_bytes(&headered).unwrap();
        assert_eq!(hd.sha1, bare.sha1);
        assert_eq!(hd.internal_name.as_deref(), Some("TEST ROM TITLE"));
    }

    #[test]
    fn rejects_tiny_files() {
        assert!(matches!(
            RomId::from_bytes(&[0u8; 16]),
            Err(RomError::TooSmall(16))
        ));
    }
}
```

Pontos novos:

- `#[cfg(test)]` — este módulo **só existe quando se roda testes**. Zero
  custo no programa final.
- `#[test]` marca cada função de teste; `assert_eq!` e `assert!` falham o
  teste com mensagem caprichada se a condição não valer.
- `.unwrap()` no teste = "se der `Err`, panique com mensagem" — aceitável em
  testes, onde você *quer* que a falha estale alto.
- `matches!(valor, padrão)` — verifica se o valor casa com um padrão (aqui:
    "é um `Err(RomError::TooSmall(16))`?"). Reaproveita o pattern matching
    da aula 4.

O teste `strips_copier_header` é uma aulinha de história do SNES: cartuchos
copiados nos anos 90 vinham com um **header de 512 bytes** de lixo no começo.
O teste garante que a ROM com header e sem header produzem o **mesmo hash** —
é isso que faz a sua ROM funcionar na estante e bater com o banco de
No-Intro/RetroAchievements.

Rodar: `cargo test` (tudo) ou `cargo test -p sixteen-domain strips` (um
teste específico).

> **Exercício 9:** teste o `human_size` que você escreveu na aula 2. Dentro
> do módulo `tests` de `rom.rs`:
>
> ```rust
> #[test]
> fn human_size_formats() {
>     let rom = vec![0u8; 0x8000];   // 32 KiB de zeros
>     let id = RomId::from_bytes(&rom).unwrap();
>     assert_eq!(id.human_size(), "32.0 KiB");
> }
> ```
>
> (Ajuste a expectativa ao formato que a sua implementação produziu.) Rode
> `cargo test -p sixteen-domain`. Se der fail, LEIA A MENSAGEM: ela mostra
> `left` e `right` coloridos. Parabéns — ciclo completo de TDD no emulador.

---

## Aula 10 — E o resto: um passeio pelos monstros do projeto

Você já tem o essencial. Estas são as regiões avançadas do código — a meta
aqui é só **reconhecer**, não dominar.

### 1. Traits: contratos de comportamento

Trait é a "interface" de Rust: um conjunto de métodos que um tipo promete
ter. Você já usou sem ver: `#[derive(Debug)]` gera a implementação do trait
`Debug`. E a aula 7 mostrou `impl Drop for Core` — `Drop` é um trait com um
método (`drop`). O projeto também usa *traits próprios* dos cores libretro
para padronizar o ciclo de vida.

### 2. FFI e `unsafe`: a fronteira com o C

O filtro de TV, `crates/ntsc/src/lib.rs`, conversa com uma biblioteca
escrita em C:

```rust
extern "C" {
    fn snes_ntsc_init(ntsc: *mut c_void, setup: *const SnesNtscSetup);
    fn snes_ntsc_blit(/* ... */);
}
```

E o crate `emulation` carrega bibliotecas `.so`/`.dylib`/`.dll` em tempo de
execução (`libloading`) e chama funções C do core. **`unsafe`** é um bloco
que diz ao Rust: "a partir daqui, eu assumo a responsabilidade que o
compilador não consegue checar". Repare como o projeto os cerca: os blocos
`unsafe` são pequenos, comentados com o porquê ("Safety: ..."), e envoltos
em funções seguras (`Core::run` etc.) que o resto do app usa sem nunca ver
um `unsafe`. É assim que se faz fronteira com C em Rust: isolar e documentar.

### 3. Concorrência: threads com canais

A checagem de atualização, `crates/app/src/update_check.rs`, não pode
travar a inicialização do app esperando a internet. Solução clássica e
segura em Rust:

```rust
// (paráfrase do cabeçalho do arquivo)
std::thread::spawn(move || {
    let aviso = check();
    sender.send(aviso).ok();   // manda pelo canal quando terminar
});
// no loop principal: receiver.try_recv() — "tem algo novo?" sem bloquear
```

Um **canal** (`mpsc` — multi-produtor, único consumidor) transporta o
resultado entre threads. E a garantia de posse da aula 7 brilha aqui: os
dados enviados pelo canal **mudam de dono** para a outra thread. Se fosse
possível two threads acessarem o mesmo valor sem coordenação, simplesmente
não compilaria. Medo de data race, em Rust, é opcional.

### 4. Macros: código que escreve código

Você já viu `format!`, `vec!`, `assert_eq!`, `matches!`, `println!`. O `!`
denota macro: algo que gera código na compilação. O projeto define até as
suas — procure `macro_rules! sym` em `core.rs` (linha 178), que encurta a
checagem repetitiva de cada símbolo vindo da biblioteca do core.

---

## Projeto final — uma contribuição de verdade

Sugestão em degraus (escolha o seu):

1. **Fácil:** o método `human_size` + teste das aulas 2 e 9. Se ainda não
   fez, é o seu "hello world" sério.
2. **Médio:** em `crates/domain/src/library.rs`, explore a função `scan` que
   varre a pasta de ROMs, e adicione um teste seu para ela.
3. **Ambicioso:** um `match` novo no `Preset` do crate `ntsc` com um preset
   de TV seu ("Trinitron 1997"?) — o arquivo tem só 232 linhas e é um ótimo
   lugar para mexer sem medo.

Depois disso, o caminho natural:

- **The Rust Book** (doc.rust-lang.org/book) — gratuito, e agora vai parecer
  revisão.
- **Rustlings** (github.com/rust-lang/rustlings) — ginástica de exercícios
  com o compilador corrigindo você.
- Voltar aqui e ler o `crates/platform/src/cabinet.rs` (o desenho do console,
  o arquivo mais visual do projeto) — se você conseguir ler 100 linhas dele
  sem pânico, o curso cumpriu o papel.

---

## Cola de bolso

| Você quer... | Em Rust | Onde ver no projeto |
|---|---|---|
| Criar variável | `let x = 5;` (imutável!) / `let mut x = 5;` | em todo lugar |
| Imprimir | `println!("oi {x}");` | — |
| Texto | `String` (dono) / `&str` (empréstimo) | `core.rs` |
| Lista | `Vec<T>` | `Frame.pixels` |
| Um-de-vários | `enum` | `Mapper`, `Button` |
| Dados com nome | `struct` | `RomId`, `Frame` |
| Escolher por valor | `match` | `environment_cb` |
| Pode não ter | `Option<T>` | `RomId.internal_name` |
| Pode falhar | `Result<T, E>` + `?` | `RomId::from_path` |
| Transformar listas | `.iter().filter().map().collect()` | `score`, `title_at` |
| Testar | `#[test] fn ...` + `assert_eq!` | fim de `rom.rs` |
| Rodar | `cargo build/run/test/check` | raiz do repo |

Bom jogo! 🎮
